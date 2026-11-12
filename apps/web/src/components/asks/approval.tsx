/**
 * A call's permission, as the person gives or withholds it: the call itself stands in the timeline, what it would do in
 * full under its line, and the person answers there or in the composer, which stays the composer — what they write in
 * it denies the call, telling the model what to do instead; Deny and Allow where it sends; allowing always, where the
 * project keeps the call's patterns, behind More. No card stands between the person and the call.
 */
import { type ReactElement, type ReactNode, useTransition } from "react";
import type { Ask } from "@octoii-works/octoii-client";
import { useClient, useThreads } from "@octoii-works/octoii-client/react";
import { Ellipsis, ShieldQuestion } from "lucide-react";
import { allowed, alwaysText, denied, type DetailLine, formOf, type Json, permissionBody } from "../../core/asks.ts";
import { useShortcut } from "../effects/shortcut.ts";
import { failed } from "../state/notices.ts";
import { Button } from "../ui/button.tsx";
import { Kbd } from "../ui/kbd.tsx";
import { Menu, MenuItem, MenuPopup, MenuTrigger } from "../ui/menu.tsx";

/** A call's permission as it waits: what it would do; the words of allowing it always, where the project keeps its patterns; whether it goes beyond the sandbox, and why; whether an answer is on its way; and the answers. */
export type Approval = {
	readonly key: string;
	/** What the call would do, line by line, past what its line names. */
	readonly lines: readonly DetailLine[];
	readonly always: string | undefined;
	readonly escalate: boolean;
	readonly justification: string | undefined;
	readonly answering: boolean;
	readonly allow: (always: boolean) => void;
	readonly deny: (reason: string) => void;
};

/** The permission `ask` of the thread `thread` asks, as the person answers it; none where it asks no permission. */
export function useApproval(thread: string, ask: Ask | undefined): Approval | undefined {
	const client = useClient();
	const project = useThreads()?.find((each) => each.id === thread)?.project ?? undefined;
	const [answering, startAnswering] = useTransition();
	if (ask === undefined) return undefined;
	const form = formOf(ask);
	if (form.kind !== "permission") return undefined;
	const answer = (value: Json): void =>
		startAnswering(async () => {
			try {
				await client.answer(thread, ask.key, value);
			} catch (error) {
				failed("The answer could not be given", error);
			}
		});
	return {
		key: ask.key,
		lines: permissionBody(form.arguments, form.access),
		always: alwaysText(form.access, project),
		escalate: form.access.escalate,
		justification: form.access.justification,
		answering,
		allow: (always) => answer(allowed(always)),
		deny: (reason) => answer(denied(reason)),
	};
}

/** How the answers are drawn: as the composer's buttons, or as words within a line. */
export type Look = "buttons" | "words";

/** The answers to `approval`: Deny, with the words `reason` gives where it gives any, and Allow; allowing always behind More. */
export function ApprovalActions(props: { readonly approval: Approval; readonly look: Look; readonly reason?: () => string }): ReactNode {
	const { approval } = props;
	const deny = (): void => approval.deny(props.reason?.() ?? "");
	if (props.look === "words") {
		return (
			<span className="flex items-center gap-2.5">
				<button type="button" disabled={approval.answering} onClick={deny} className={WORD}>
					Deny
				</button>
				<button type="button" disabled={approval.answering} onClick={() => approval.allow(false)} className={ALLOW_WORD}>
					Allow
				</button>
				<More approval={approval} trigger={<button type="button" className={WORD} aria-label="More ways to answer" />} label="More" />
			</span>
		);
	}
	return (
		<span className="flex shrink-0 items-center gap-1.5">
			<Button size="xs" variant="outline" disabled={approval.answering} onClick={deny}>
				Deny
			</Button>
			<Button size="xs" variant="default" disabled={approval.answering} onClick={() => approval.allow(false)}>
				Allow <Kbd className="bg-transparent text-primary-foreground opacity-75 pointer-coarse:hidden">⌘↵</Kbd>
			</Button>
			<More approval={approval} trigger={<Button size="icon-xs" variant="outline" aria-label="More ways to answer" />} label={<Ellipsis />} />
		</span>
	);
}

/** A word that answers, quiet; the one that allows, in the words' own ink. */
const WORD = "cursor-pointer text-muted-foreground hover:text-foreground disabled:opacity-64";
const ALLOW_WORD = "cursor-pointer font-medium text-foreground hover:opacity-80 disabled:opacity-64";

/** Allowing always, behind More; nothing where the project keeps no patterns of the call. */
function More(props: { readonly approval: Approval; readonly trigger: ReactElement; readonly label: ReactNode }): ReactNode {
	const { approval } = props;
	if (approval.always === undefined) return null;
	return (
		<Menu>
			<MenuTrigger disabled={approval.answering} render={props.trigger}>
				{props.label}
			</MenuTrigger>
			<MenuPopup side="top" align="end">
				<MenuItem onClick={() => approval.allow(true)}>{approval.always}</MenuItem>
			</MenuPopup>
		</Menu>
	);
}

/** What waits on the person under a call's line: that it waits, and how far it would reach; why, where the model said; then the answers, in words. */
export function ApprovalLine(props: { readonly approval: Approval }): ReactNode {
	const { approval } = props;
	return (
		<div className="flex flex-col gap-1 text-xs">
			{approval.justification === undefined ? null : <p className="text-muted-foreground">{approval.justification}</p>}
			<p className="flex flex-wrap items-center gap-x-2 gap-y-1">
				<span className="inline-flex items-center gap-1 text-warning-foreground">
					<ShieldQuestion className="size-3 shrink-0" />
					{approval.escalate ? "Waiting for you · beyond the sandbox" : "Waiting for you"}
				</span>
				<span aria-hidden className="text-faint-foreground">
					·
				</span>
				<ApprovalActions approval={approval} look="words" />
			</p>
		</div>
	);
}

/** Allowing with ⌘↵ wherever the focus is, while the approval waits in the composer. */
export function useAllowShortcut(approval: Approval): void {
	useShortcut("mod+Enter", () => approval.allow(false));
}

/** What a call does, line by line, an edit's lines taken out and put in marked as a diff marks them. */
export function PermissionDetail(props: { readonly lines: readonly DetailLine[] }): ReactNode {
	return (
		<pre className="well max-h-48 overflow-auto px-3 py-2 font-mono text-xs leading-5 whitespace-pre-wrap [overflow-wrap:anywhere]">
			{props.lines.map((line) => (
				<div key={line.number} className={LINE_CLASSES[line.kind]}>
					{line.kind === "removed" ? "- " : line.kind === "added" ? "+ " : ""}
					{line.text}
				</div>
			))}
		</pre>
	);
}

/** How each line of a call's detail reads. */
const LINE_CLASSES: Readonly<Record<DetailLine["kind"], string>> = {
	text: "text-foreground",
	removed: "bg-error-surface text-error-foreground",
	added: "bg-success-surface text-success-foreground",
};
