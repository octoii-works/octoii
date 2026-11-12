/**
 * The person's messages that wait in a thread's inbox, docked over the composer: each taken into the turn that works as its
 * next step begins, which it steers, or begins the next turn; each rising in as it is sent, with its text, its links by
 * their words, a pen that takes it back into the composer to be written again — words alone, as files go no way back —
 * and a cross that takes it back while it waits.
 */
import type { ReactNode } from "react";
import type { Inbound, InputContent } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { CornerDownRight, Pencil, X } from "lucide-react";
import { linksAsWords } from "../../core/links.ts";
import { inputText, threadOfActor } from "../../core/timeline.ts";
import { useDraft } from "../state/draft.ts";
import { failed } from "../state/notices.ts";

/** The person's messages waiting in `thread`'s inbox — not what a thread delivered, which is not theirs to take back; nothing where none waits. */
export function Queued(props: { readonly thread: string; readonly inbox: readonly (readonly [string, Inbound])[] }): ReactNode {
	const queued = props.inbox.flatMap(([id, inbound]) =>
		inbound.type === "input" && threadOfActor(inbound.actor) === undefined ? [{ id, content: inbound.content }] : [],
	);
	if (queued.length === 0) return null;
	return (
		<div className="mx-3 mb-[-1px] flex flex-col gap-0.5 rounded-t-lg border border-b-0 border-border bg-card px-2 py-1.5">
			{queued.map((input) => (
				<QueuedRow key={input.id} thread={props.thread} id={input.id} content={input.content} />
			))}
		</div>
	);
}

/** A row of the strip's look, rising in as it comes. */
const ROW =
	"flex h-control-sm items-center gap-2 rounded-md px-1.5 text-ui transition-[opacity,translate] duration-200 hover:bg-accent starting:translate-y-1 starting:opacity-0 motion-reduce:transition-none";

/** A message of the person's waiting in the inbox: written again in the composer, its words before what the composer holds; or taken back. */
function QueuedRow(props: { readonly thread: string; readonly id: string; readonly content: readonly InputContent[] }): ReactNode {
	const client = useClient();
	const draft = useDraft({ thread: props.thread });
	const words = inputText(props.content);
	const wordsAlone = props.content.every((part) => part.type === "input_text");
	const takeBack = (): Promise<boolean> =>
		client.withdraw(props.thread, props.id).catch((error: unknown) => {
			failed("The message could not be taken back", error);
			return false;
		});
	const edit = (): void =>
		void takeBack().then((taken) => {
			if (taken) draft.setText(draft.text.trim() === "" ? words : `${words}\n\n${draft.text}`);
		});
	return (
		<div className={ROW}>
			<CornerDownRight className="size-3.5 shrink-0 text-muted-foreground" />
			<span className="min-w-0 flex-1 truncate text-foreground">{linksAsWords(words) || "A message"}</span>
			<span className="text-xs text-muted-foreground">Joins at the next step</span>
			{wordsAlone ? (
				<button type="button" aria-label="Edit" title="Edit" onClick={edit} className={ICON}>
					<Pencil className="size-3.5" />
				</button>
			) : null}
			<button type="button" aria-label="Take back" title="Take back" onClick={() => void takeBack()} className={ICON}>
				<X className="size-3.5" />
			</button>
		</div>
	);
}

/** A row's quiet icon button. */
const ICON = "flex size-5 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground";
