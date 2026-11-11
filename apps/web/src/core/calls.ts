/**
 * A call as its tool told it, read: what it did, as one of Siphon's own tools tells it typed — how a command ended, what
 * a write or an edit changed, the lines a read held; the text its tool gave the model, and the notes after it in
 * brackets, shown as they are; an edit's diff and what a search found, in the formats every diff and grep write; and
 * the arguments it was given, as fields.
 */
import type { CommandEnd, JsonValue, Structured } from "@octoii-works/octoii-client";
import { matches } from "./schema.ts";
import { Type } from "typebox";

/** How a call stands: running, waiting for the person's approval, done, or not done — failed, stopped, or rejected. */
export type CallState = "running" | "waiting" | "done" | "failed" | "stopped" | "rejected";

/**
 * A call as its tool's readers take it: its tool, its arguments as they were written, what its tool told the model and
 * what it told typed — none before it ended — and how it stands.
 */
export type ToolCall = {
	readonly tool: string;
	readonly arguments: string;
	readonly output: string | undefined;
	readonly structured: Structured | undefined;
	readonly state: CallState;
};

/** What one of Siphon's own tools told typed of a call; none for an MCP server's, named `server.tool`, or where it told none. */
export function structuredOf(tool: string, structured: JsonValue | null | undefined): Structured | undefined {
	if (tool.includes(".") || structured === undefined || structured === null) return undefined;
	// SAFETY: Siphon's own tools tell what a call did as `Structured`, as the protocol's types are made from theirs.
	return structured as Structured;
}

/** What a tool told: its text, and the notes after it, each a line in brackets. */
export type Told = { readonly text: string; readonly notes: readonly string[] };

/** `output` as its text and the notes after it, read from its end, the text left as it is. */
export function toldOf(output: string): Told {
	const notes: string[] = [];
	let end = output.length;
	while (end > 0) {
		const start = output.lastIndexOf("\n", end - 1) + 1;
		const note = /^\[(.*)\]$/.exec(output.slice(start, end));
		if (note === null) break;
		notes.unshift(note[1] ?? "");
		end = Math.max(0, start - 1);
	}
	return { text: output.slice(0, end), notes };
}

/** What a command told, but the note that it exited with nought: a command that went well says nothing of how it exited. */
export function commandToldOf(output: string): Told {
	const told = toldOf(output);
	return { text: told.text, notes: told.notes.filter((note) => note !== "exit 0") };
}

/** How the command `call` ran ended, as its tool told; none while it runs, or where its tool tells nothing. */
export function endOf(call: ToolCall): CommandEnd | undefined {
	return call.structured?.type === "ran" ? call.structured.ended : undefined;
}

/** What an edit or a write added and took out, in lines. */
export type Change = { readonly added: number; readonly removed: number };

/** What the edit or the write `call` changed, as its tool told; none for any other call, or one that did not go through. */
export function changeOf(call: ToolCall): Change | undefined {
	if (call.structured?.type !== "changed" || call.state !== "done") return undefined;
	return { added: call.structured.additions, removed: call.structured.deletions };
}

/** The diff the edit `call` gave, from its first hunk on — its head, which names the file its line names, left out; none for any other call, or one that did not go through. */
export function hunksOf(call: ToolCall): string | undefined {
	if (call.tool !== "edit" || call.output === undefined || call.state !== "done") return undefined;
	const lines = toldOf(call.output).text.split("\n");
	return lines
		.slice(
			Math.max(
				0,
				lines.findIndex((line) => line.startsWith("@@")),
			),
		)
		.join("\n");
}

/** The lines a read held, "lines 1–200"; none where it held a whole file. */
export function spanOf(call: ToolCall): string | undefined {
	if (call.structured?.type !== "read") return undefined;
	const { from, to, total } = call.structured;
	if (total !== undefined && from <= 1) return undefined;
	return `lines ${String(from)}–${String(to)}`;
}

/** A place a search found: a file's path, and the line and its words where it found a line in it. */
export type Found = { readonly path: string; readonly line: number | undefined; readonly text: string | undefined };

/** What the search `call` found: a glob's paths, a grep's matching lines — its lines around them left out. */
export function foundOf(call: ToolCall): readonly Found[] {
	if (call.output === undefined || call.state !== "done") return [];
	const lines = toldOf(call.output)
		.text.split("\n")
		.filter((line) => line !== "");
	if (call.tool === "glob") return lines.map((path) => ({ path, line: undefined, text: undefined }));
	return lines.flatMap((line) => {
		const match = /^(.+?):(\d+): (.*)$/.exec(line);
		return match === null ? [] : [{ path: match[1] ?? "", line: Number(match[2]), text: match[3] ?? "" }];
	});
}

/** A field of a call's arguments: its name, and its value as text — a string as it is, anything else as JSON. */
export type Field = { readonly name: string; readonly value: string };

const FieldsSchema = Type.Record(Type.String(), Type.Unknown());

/** The fields of the arguments `text` holds, in the order they were written; none where they are no JSON object. */
export function fieldsOf(text: string): readonly Field[] | undefined {
	try {
		const given: unknown = JSON.parse(text);
		if (!matches(FieldsSchema, given) || Array.isArray(given)) return undefined;
		return Object.entries(given).map(([name, value]) => ({ name, value: typeof value === "string" ? value : JSON.stringify(value) }));
	} catch {
		return undefined;
	}
}

/** Arguments as a person reads them: their JSON laid out, a line for each field; as they were written where they do not parse. */
export function prettyArguments(text: string): string {
	try {
		return JSON.stringify(JSON.parse(text), null, 2);
	} catch {
		return text;
	}
}
