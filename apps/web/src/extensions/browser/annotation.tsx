import { type FormEvent, type ReactNode, useRef, useState } from "react";
import type { ElementFeedback } from "@octoii-works/octoii-client";
import { useClient } from "@octoii-works/octoii-client/react";
import { ANNOTATION_LIMIT, elementContent, elementSummary } from "../../core/element-feedback.ts";
import { focusThreadMessage } from "../../components/effects/focus-message.ts";
import { attachElementFeedback } from "../../components/state/draft.ts";
import { failed } from "../../components/state/notices.ts";
import { Button } from "../../components/ui/button.tsx";

const WIDTH = 304;
const HEIGHT = 170;
const GAP = 8;

/** Writes a note beside its target, while the captured viewport is still on screen. */
export function AnnotationEditor(props: {
	readonly feedback: ElementFeedback;
	readonly onClose: () => void;
	readonly onSent: (delivery: string) => void;
}): ReactNode {
	const { feedback, onClose, onSent } = props;
	const client = useClient();
	const [comment, setComment] = useState("");
	const [sending, setSending] = useState(false);
	const pending = useRef(false);
	const note = { ...feedback, comment: comment.trim() };
	const rect = feedback.element.rect;
	const left = Math.max(GAP, Math.min(rect.x, feedback.viewport.width - WIDTH - GAP));
	const below = rect.y + rect.height + GAP;
	const above = rect.y - HEIGHT - GAP;
	const top = Math.max(GAP, Math.min(below + HEIGHT <= feedback.viewport.height || above < GAP ? below : above, feedback.viewport.height - HEIGHT));

	const attach = (event: FormEvent<HTMLFormElement>): void => {
		event.preventDefault();
		if (pending.current) return;
		attachElementFeedback(note);
		onClose();
		focusThreadMessage(feedback.thread);
	};
	const send = (): void => {
		if (pending.current) return;
		pending.current = true;
		setSending(true);
		void client.deliver(feedback.thread, elementContent(note)).then(
			(delivery) => {
				onClose();
				onSent(delivery);
			},
			(error: unknown) => {
				pending.current = false;
				setSending(false);
				failed("The annotation could not be sent", error);
			},
		);
	};

	return (
		<>
			<div
				aria-hidden
				className="pointer-events-none absolute z-10 rounded-sm outline-2 outline-ring"
				style={{ left: rect.x, top: rect.y, width: rect.width, height: rect.height }}
			/>
			<form
				aria-label="Annotate the selected element"
				className="pointer-events-auto absolute z-10 flex max-w-[calc(100%-1rem)] flex-col gap-2 rounded-lg border border-border bg-background p-3 shadow-raised"
				style={{ left, top, width: WIDTH }}
				onSubmit={attach}
			>
				<span className="truncate text-xs font-medium" title={elementSummary(feedback)}>
					{elementSummary(feedback)}
				</span>
				<textarea
					autoFocus
					aria-label="Annotation"
					placeholder="What should change?"
					value={comment}
					maxLength={ANNOTATION_LIMIT}
					disabled={sending}
					onChange={(event) => setComment(event.currentTarget.value)}
					onKeyDown={(event) => {
						if (event.nativeEvent.isComposing || event.key !== "Enter" || event.shiftKey) return;
						event.preventDefault();
						event.stopPropagation();
						if (event.metaKey || event.ctrlKey) send();
						else event.currentTarget.form?.requestSubmit();
					}}
					className="min-h-16 resize-y rounded-md border border-border bg-background p-2 text-ui outline-none focus-visible:border-ring"
				/>
				<div className="flex flex-wrap justify-end gap-1">
					<Button variant="ghost" size="xs" disabled={sending} onClick={onClose}>
						Cancel
					</Button>
					<Button variant="outline" size="xs" type="submit" aria-label="Attach annotation to draft" disabled={sending}>
						Attach
					</Button>
					<Button size="xs" aria-label="Send annotation" disabled={sending} onClick={send}>
						{sending ? "Sending…" : "Send"}
					</Button>
				</div>
			</form>
		</>
	);
}
