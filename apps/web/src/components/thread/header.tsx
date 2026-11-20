/**
 * The page's header: the frame's button that shows the sidebar, where it is not beside the page; the project and the
 * thread's title as a breadcrumb, the project by its mark alone where the page's column is narrow, so that the title
 * has the room; a pill of how the thread stands while it works, waits on the person or failed; and on its right what
 * the page puts there — a thread's, the frame's button that shows its panel.
 */
import type { ReactNode } from "react";
import { type Project, THREAD_STATUS_LABELS, type ThreadStatus } from "../../core/threads.ts";
import { ProjectMark } from "../files/project-mark.tsx";
import { ShowSidebar } from "../frame/frame.tsx";
import { Pill } from "../ui/pill.tsx";
import { StatusMark } from "./status-mark.tsx";

/** What the header shows: the thread's project, title and standing, and what stands on its right. */
export type HeaderProps = {
	readonly project?: Project;
	readonly title: string;
	readonly status?: ThreadStatus | undefined;
	readonly actions?: ReactNode;
};

/** The header. */
export function ChatHeader(props: HeaderProps): ReactNode {
	return (
		<header className="window-toolbar flex h-[var(--topbar-height)] shrink-0 items-center gap-2 bg-background px-3 @xl/pane:px-5">
			<ShowSidebar className="-ml-1" />
			<div className="flex min-w-0 flex-1 items-center gap-1.5 text-ui">
				{props.project === undefined ? null : (
					<>
						<ProjectMark project={props.project} />
						<span className="max-w-40 truncate text-muted-foreground @max-xl/pane:hidden">{props.project.name}</span>
						<span className="text-faint-foreground @max-xl/pane:hidden">/</span>
					</>
				)}
				<h2 className="min-w-0 truncate font-medium text-foreground">{props.title}</h2>
				{props.status === undefined ? null : <StatusPill status={props.status} />}
			</div>
			<div role="group" aria-label="View controls" className="flex shrink-0 items-center gap-1">
				{props.actions}
			</div>
		</header>
	);
}

/** How the thread stands, as a pill. */
function StatusPill(props: { readonly status: ThreadStatus }): ReactNode {
	const { status } = props;
	if (status === "ready") return null;
	const tone = { preparing: "waiting", asks: "waiting", working: "running", failed: "failed", unavailable: "failed" } as const;
	return (
		<Pill tone={tone[status]} icon={<StatusMark status={status} />}>
			{THREAD_STATUS_LABELS[status]}
		</Pill>
	);
}
