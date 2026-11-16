/** Saved browser selections held in the draft until the person sends their message. */
import type { ReactNode } from "react";
import type { ElementFeedback } from "@octoii-works/octoii-client";
import { X } from "lucide-react";
import { elementSummary } from "../../core/element-feedback.ts";
import { PictureStrip } from "../thread/pictures.tsx";

export function ElementFeedbackCards(props: {
	readonly feedback: readonly ElementFeedback[];
	readonly onRemove: (feedback: ElementFeedback) => void;
}): ReactNode {
	return props.feedback.map((feedback) => {
		const summary = elementSummary(feedback);
		return (
			<div
				key={`${feedback.screenshot.ref}:${String(feedback.captured_at)}`}
				className="flex min-w-0 items-start gap-2 rounded-md border border-border bg-muted p-2"
			>
				<PictureStrip
					thread={feedback.thread}
					pictures={[{ kind: "kept", ref: feedback.screenshot.ref, name: `Captured ${summary}` }]}
					className="shrink-0 p-0"
					tileClassName="h-16 w-24 min-w-0 object-contain"
				/>
				<div className="flex min-w-0 flex-1 flex-col gap-1 py-0.5 text-xs">
					<span className="font-medium text-foreground">Selected element</span>
					<span className="line-clamp-2 break-words">{summary}</span>
					<span title={feedback.url} className="truncate text-muted-foreground">
						{feedback.url}
					</span>
				</div>
				<button
					type="button"
					aria-label={`Remove selected ${summary}`}
					onClick={() => props.onRemove(feedback)}
					className="flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
				>
					<X className="size-3.5" />
				</button>
			</div>
		);
	});
}
