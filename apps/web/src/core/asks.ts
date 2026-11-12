/**
 * What waits on the person in a thread, as the page asks it: each request a fiber made that the person answers, read
 * from the JSON Schema its answer fits. Trust in what a folder's configuration would run is asked with the servers it
 * names; a call's permission with what it does, and to what; a model's questions one by one, each with its choices and
 * the person's own words; a union of objects each named by one constant, such as `{ decision: "once" }`, is a choice of
 * them, each with the fields it takes; an object's own fields are asked one by one; a schema of neither shape is shown,
 * not asked.
 */
import type { Ask, JsonValue } from "@octoii-works/octoii-client";
import { matches } from "./schema.ts";
import { type Static, Type } from "typebox";
import { folderName } from "./threads.ts";

/** A JSON value, as an answer is made of. */
export type Json = JsonValue;

/**
 * A field an answer takes: where its value goes, such as `["reason"]` or `["content", "email"]`, its label, what it
 * means, whether it must be given, and what it takes: text, a number, yes or no, or one of some values.
 */
export type Field = {
	readonly path: readonly string[];
	readonly label: string;
	readonly description?: string;
	readonly required: boolean;
	readonly takes: { readonly kind: "text" | "number" | "boolean" } | { readonly kind: "one"; readonly values: readonly string[] };
};

/** One answer a request offers: the constant that names it, its label, and the fields it takes. */
export type Choice = { readonly value: string; readonly label: string; readonly fields: readonly Field[] };

/** A server a folder's configuration would run, as the person is shown it. */
export type ServerShown = Static<typeof ServerSchema>;

/**
 * What a call does to the machine, as its permission is asked: its action, read from its tool; what it acts on — the
 * files it edits, the commands it runs, the tool by its name; the patterns an always grants the thread's project; and
 * whether it asks to run beyond the sandbox, and why.
 */
export type Access = {
	readonly action: "edit" | "command" | "tool";
	readonly resources: readonly string[];
	readonly save: readonly string[];
	readonly escalate: boolean;
	readonly justification: string | undefined;
};

/** A question a model asks: its words, a short label, the choices it offers, and whether several may be chosen. */
export type Question = Static<typeof QuestionSchema>;

/** What a request asks, by its form. */
export type AskForm =
	| { readonly kind: "trust"; readonly folder: string; readonly servers: readonly ServerShown[] }
	| { readonly kind: "permission"; readonly tool: string; readonly arguments: unknown; readonly access: Access }
	| { readonly kind: "question"; readonly questions: readonly Question[] }
	| { readonly kind: "choices"; readonly key: string; readonly choices: readonly Choice[] }
	| { readonly kind: "fields"; readonly fields: readonly Field[] }
	| { readonly kind: "shown" };

/** What the person gave of each field of an answer, by its path joined with `.`. */
export type Given = Readonly<Record<string, Json>>;

/** The name of Siphon's request for the person's trust in what a folder's configuration names to run. */
const TRUST = "siphon.trust";
/** The name of Siphon's request for the person's permission for a call. */
const PERMISSION = "siphon.permission";
/** The name of Siphon's request for a call the person answers, as the `question` tool's are. */
const ASKS = "siphon.asks";
/** The tool whose calls ask the person questions. */
const QUESTION_TOOL = "question";

const Nullable = <Schema extends Parameters<typeof Type.Optional>[0]>(schema: Schema) => Type.Optional(Type.Union([schema, Type.Null()]));
const GrantsSchema = Type.Object({ commands: Type.Array(Type.String()), paths: Type.Array(Type.String()), tools: Type.Array(Type.String()) });
const PermissionAskedSchema = Type.Object({
	shown: Type.Object({ tool: Type.String(), arguments: Type.String(), does: Type.Array(Type.String()), justification: Nullable(Type.String()) }),
	always: GrantsSchema,
});
const QuestionSchema = Type.Object({
	question: Type.String(),
	header: Nullable(Type.String()),
	options: Nullable(Type.Array(Type.Object({ label: Type.String(), description: Nullable(Type.String()) }))),
	multiple: Nullable(Type.Boolean()),
});
const QuestionsAskedSchema = Type.Object({ tool: Type.Literal(QUESTION_TOOL), arguments: Type.Object({ questions: Type.Array(QuestionSchema) }) });

const ServerSchema = Type.Object({ name: Type.String(), runs: Type.String() });
const TrustAskedSchema = Type.Object({ folder: Type.String(), servers: Type.Array(ServerSchema) });
const EscalatesSchema = Type.Object({ escalate: Type.Literal(true) });
const ObjectSchema = Type.Object({ properties: Type.Record(Type.String(), Type.Unknown()), required: Type.Optional(Type.Array(Type.String())) });
const UnionSchema = Type.Object({ anyOf: Type.Array(Type.Unknown()) });
const PropertySchema = Type.Object({
	type: Type.Optional(Type.String()),
	title: Type.Optional(Type.String()),
	description: Type.Optional(Type.String()),
	enum: Type.Optional(Type.Array(Type.Unknown())),
});
const ConstSchema = Type.Object({ const: Type.String() });

type ObjectShape = Static<typeof ObjectSchema>;

/** The form of what `ask` asks: by its request's name and what it was made with, else by its answer's schema. */
export function formOf(ask: Ask): AskForm {
	const { name, payload, answer_schema: answer } = ask;
	if (name === TRUST && matches(TrustAskedSchema, payload)) return { kind: "trust", folder: payload.folder, servers: payload.servers };
	if (name === PERMISSION && matches(PermissionAskedSchema, payload)) return permissionOf(payload);
	if (name === ASKS && matches(QuestionsAskedSchema, payload)) return { kind: "question", questions: payload.arguments.questions };
	const members = matches(UnionSchema, answer) ? answer.anyOf.filter((member) => matches(ObjectSchema, member)) : [];
	const offered = members.length > 0 ? members : matches(ObjectSchema, answer) ? [answer] : [];
	const choices = choicesOf(offered);
	if (choices !== undefined) return choices;
	const [only] = offered;
	return offered.length === 1 && only !== undefined ? { kind: "fields", fields: fieldsOf(only, []) } : { kind: "shown" };
}

/** A call's permission, from what Siphon asks: its tool and its arguments, what it does, and what an always grants. */
function permissionOf(asked: Static<typeof PermissionAskedSchema>): AskForm {
	const { shown: call, always } = asked;
	const args = argumentsOf(call.arguments);
	const action = call.tool === "shell" ? "command" : call.tool === "write" || call.tool === "edit" ? "edit" : "tool";
	const access: Access = {
		action,
		// What it does reads as "edits <path>", "runs <command>": what it acts on is past the verb.
		resources: call.does.map((does) => does.replace(/^\S+\s+/, "")),
		save: [...always.commands, ...always.paths, ...always.tools],
		escalate: matches(EscalatesSchema, args),
		justification: call.justification ?? undefined,
	};
	return { kind: "permission", tool: call.tool, arguments: args, access };
}

/** A call's arguments as the model wrote them, read as JSON where they are; else as they were written. */
function argumentsOf(written: string): Json {
	try {
		// SAFETY: what JSON.parse gives is JSON.
		return JSON.parse(written) as Json;
	} catch {
		return written;
	}
}

/** The answers objects offer, each named by a constant of the same property; none for another shape. */
function choicesOf(offered: readonly ObjectShape[]): AskForm | undefined {
	const named = offered.map((member) => ({ member, key: constKeyOf(member) }));
	const key = named[0]?.key;
	if (key === undefined || named.some((each) => each.key !== key)) return undefined;
	const choices = named.map(({ member }): Choice => {
		const property = member.properties[key];
		const value = matches(ConstSchema, property) ? property.const : "";
		const label = matches(PropertySchema, property) && property.title !== undefined ? property.title : spoken(value);
		return { value, label, fields: fieldsOf(member, [], key) };
	});
	return { kind: "choices", key, choices };
}

/** The one property of an object's schema that is a string constant; none where it has none or several. */
function constKeyOf(schema: ObjectShape): string | undefined {
	const keys = Object.keys(schema.properties).filter((name) => matches(ConstSchema, schema.properties[name]));
	return keys.length === 1 ? keys[0] : undefined;
}

/** The fields an object's schema takes, but `left`, an object's own taken one level down, under `at`. */
function fieldsOf(schema: ObjectShape, at: readonly string[], left?: string): readonly Field[] {
	const required = new Set(schema.required ?? []);
	return Object.entries(schema.properties).flatMap(([name, property]): readonly Field[] => {
		if (name === left || !matches(PropertySchema, property)) return [];
		if (property.type === "object" && at.length === 0 && matches(ObjectSchema, property)) return fieldsOf(property, [name]);
		const takes = takesOf(property);
		if (takes === undefined) return [];
		return [
			{
				path: [...at, name],
				label: property.title ?? spoken(name),
				...(property.description === undefined ? {} : { description: property.description }),
				required: required.has(name),
				takes,
			},
		];
	});
}

/** What a property takes, where a field can give it: text, a number, yes or no, or one of its values. */
function takesOf(property: Static<typeof PropertySchema>): Field["takes"] | undefined {
	const values = (property.enum ?? []).filter((value) => typeof value === "string");
	if (values.length > 0) return { kind: "one", values };
	switch (property.type) {
		case "string":
			return { kind: "text" };
		case "number":
		case "integer":
			return { kind: "number" };
		case "boolean":
			return { kind: "boolean" };
		default:
			return undefined;
	}
}

/** Whether a field was left without a value. */
export function isEmpty(value: Json | undefined): boolean {
	return value === undefined || value === null || (typeof value === "string" && value.trim() === "");
}

/** An answer of fields: each field given at its path, text trimmed, under `base`. */
export function fieldsAnswer(fields: readonly Field[], given: Given, base: Readonly<Record<string, Json>> = {}): Json {
	const answer: Record<string, Json> = { ...base };
	for (const field of fields) {
		const value = given[field.path.join(".")];
		const [first, second] = field.path;
		if (value === undefined || isEmpty(value) || first === undefined) continue;
		const kept = typeof value === "string" ? value.trim() : value;
		if (second === undefined) answer[first] = kept;
		else {
			const inner = answer[first];
			answer[first] = { ...(typeof inner === "object" && inner !== null && !Array.isArray(inner) ? inner : {}), [second]: kept };
		}
	}
	return answer;
}

/** An answer of a choice: its constant under its property, and each field given at its path. */
export function choiceAnswer(property: string, choice: Choice, given: Given): Json {
	return fieldsAnswer(choice.fields, given, { [property]: choice.value });
}

/** Whether a choice's or an object's fields are given as they must be. */
export function missingField(fields: readonly Field[], given: Given): boolean {
	return fields.some((field) => field.required && field.takes.kind !== "boolean" && isEmpty(given[field.path.join(".")]));
}

/** A line of what a call asks to do, as its card shows it: its number, which names it, and its text, plain or taken out or put in by an edit. */
export type DetailLine = { readonly number: number; readonly kind: "text" | "removed" | "added"; readonly text: string };

/** A line before it is numbered. */
type Unnumbered = Omit<DetailLine, "number">;

const CommandArgumentsSchema = Type.Object({ command: Type.String() });
const EditArgumentsSchema = Type.Object({ old: Type.String(), new: Type.String() });
const WriteArgumentsSchema = Type.Object({ content: Type.String() });

/** What a call asks the person, in words: to run a command — beyond the sandbox, where it asks to — to write or edit a file, or to use a tool. */
export function permissionTitle(tool: string, access: Access): string {
	switch (access.action) {
		case "command":
			return access.escalate ? "Run this beyond the sandbox?" : "Run this command?";
		case "edit":
			return tool === "write" ? "Write this file?" : "Edit this file?";
		case "tool":
			return `Use ${tool}?`;
	}
}

/** What trust in what `folder` runs asks, in words. */
export function trustTitle(folder: string): string {
	return `Trust what ${folderName(folder)} runs?`;
}

/** What `ask` asks, in a line: trust in what a folder runs, a call's permission, a question's heading, else the request's name. */
export function askTitle(ask: Ask): string {
	const form = formOf(ask);
	switch (form.kind) {
		case "trust":
			return trustTitle(form.folder);
		case "permission":
			return permissionTitle(form.tool, form.access);
		case "question":
			return form.questions[0]?.header ?? form.questions[0]?.question ?? ask.name;
		case "choices":
		case "fields":
		case "shown":
			return ask.name;
	}
}

/**
 * What a call asks to do, line by line: the command line it runs; the file it edits — as the model writes it, relative
 * within the thread's workspace — then its change, what it takes out and puts in, or what a write puts in whole; the
 * tool it calls, then its arguments.
 */
export function permissionDetail(tool: string, args: unknown, access: Access, workspace: string | undefined): readonly DetailLine[] {
	const head = access.action === "edit" ? shownPath(access.resources[0] ?? "", workspace) : access.action === "tool" ? tool : undefined;
	return numbered([...(head === undefined ? [] : [plain(head)]), ...bodyOf(args, access)]);
}

/** What a call asks to do, line by line, under the line that names what it acts on: its command line, its change, or its arguments. */
export function permissionBody(args: unknown, access: Access): readonly DetailLine[] {
	return numbered(bodyOf(args, access));
}

/** `lines`, numbered from nought. */
function numbered(lines: readonly Unnumbered[]): readonly DetailLine[] {
	return lines.map((line, number) => ({ ...line, number }));
}

/** A line of text, as a detail shows it. */
function plain(line: string): Unnumbered {
	return { kind: "text", text: line };
}

/** What a call asks to do, past what it acts on: the command line it runs, an edit's change, or a tool's arguments. */
function bodyOf(args: unknown, access: Access): readonly Unnumbered[] {
	switch (access.action) {
		case "command":
			return (matches(CommandArgumentsSchema, args) ? args.command : access.resources.join("\n")).split("\n").map(plain);
		case "edit":
			return changeOf(args);
		case "tool":
			return JSON.stringify(args, null, 2).split("\n").map(plain);
	}
}

/** An absolute path as the model writes it: relative within `workspace`, `.` for the workspace itself, else whole. */
export function shownPath(path: string, workspace: string | undefined): string {
	if (workspace === undefined) return path;
	if (path === workspace) return ".";
	const within = workspace.endsWith("/") ? workspace : `${workspace}/`;
	return path.startsWith(within) ? path.slice(within.length) : path;
}

/** The lines an edit takes out and puts in, or a write puts in whole; none of another call. */
function changeOf(args: unknown): readonly Unnumbered[] {
	const lines = (kind: DetailLine["kind"], body: string): readonly Unnumbered[] => body.split("\n").map((line) => ({ kind, text: line }));
	if (matches(EditArgumentsSchema, args)) return [...lines("removed", args.old), ...lines("added", args.new)];
	if (matches(WriteArgumentsSchema, args)) return lines("added", args.content.replace(/\n$/, ""));
	return [];
}

/** What an always answer grants the thread's project, in words; none where nothing of the call could be granted, or the thread has no project. */
export function alwaysText(access: Access, project: string | undefined): string | undefined {
	if (access.save.length === 0 || project === undefined) return undefined;
	const where = folderName(project);
	return access.action === "edit" ? `Allow every edit in ${where}` : `Always allow ${access.save.join(", ")} in ${where}`;
}

/** What the person gave of a question so far: the choices' labels they chose, and words of their own. */
export type Draft = { readonly chosen: readonly string[]; readonly own: string };

/** A question nothing is given of yet. */
export const UNANSWERED: Draft = { chosen: [], own: "" };

/** `draft` with the choice `label` taken: the one answer of a question of one choice, else toggled among the rest. */
export function choose(question: Question, draft: Draft, label: string): Draft {
	if (question.multiple === true) {
		return { ...draft, chosen: draft.chosen.includes(label) ? draft.chosen.filter((each) => each !== label) : [...draft.chosen, label] };
	}
	return { chosen: draft.chosen[0] === label ? [] : [label], own: "" };
}

/** `draft` with the person's own words: of a question of one choice, its answer in place of a choice. */
export function write(question: Question, draft: Draft, own: string): Draft {
	if (question.multiple === true || own.trim() === "") return { ...draft, own };
	return { chosen: [], own };
}

/** The answer of questions the person dismissed, answering none. */
export const DISMISSED: Json = { dismissed: true };

/** The answer to the questions, from what the person gave of each: the labels chosen and their own words, none for one left unanswered. */
export function questionsAnswer(drafts: readonly Draft[]): Json {
	return {
		answers: drafts.map((draft) => {
			const own = draft.own.trim();
			if (draft.chosen.length === 0 && own === "") return null;
			return { chosen: [...draft.chosen], ...(own === "" ? {} : { text: own }) };
		}),
	};
}

/** The person's permission for a call: just once, or always in the thread's project as its patterns say. */
export function allowed(always: boolean): Json {
	return { decision: "allow", always };
}

/** The person's refusal of a call, with what they wrote for the model of what to do instead, where they wrote any. */
export function denied(reason: string): Json {
	return reason.trim() === "" ? { decision: "deny" } : { decision: "deny", reason: reason.trim() };
}

/** A name as people read it: `not_now` is "Not now". */
function spoken(name: string): string {
	const words = name.replaceAll(/[_-]+/g, " ").trim();
	return words.charAt(0).toUpperCase() + words.slice(1);
}
