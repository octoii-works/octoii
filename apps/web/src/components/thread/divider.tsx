/**
 * A moment of the thread that is neither the person's words nor the model's work — Siphon's own notice, the window
 * summed up — as a line across the column: what happened, and quieter after it what came of it, a rule on either side.
 */
import type { ReactNode } from "react";

/** The divider of `event`, its `detail` after it, led by `icon` where it has one. */
export function SystemDivider(props: { readonly event: string; readonly detail?: string | undefined; readonly icon?: ReactNode }): ReactNode {
	return (
		<div className="flex min-w-0 items-center gap-2 py-2 text-2xs text-muted-foreground">
			<span aria-hidden="true" className="h-px flex-1 bg-border" />
			<span className="flex min-w-0 items-center gap-1.5 [&>svg]:size-3 [&>svg]:shrink-0">
				{props.icon}
				<span className="shrink-0 font-medium">{props.event}</span>
				{props.detail === undefined ? null : <span className="min-w-0 truncate opacity-70">{`· ${props.detail}`}</span>}
			</span>
			<span aria-hidden="true" className="h-px flex-1 bg-border" />
		</div>
	);
}
