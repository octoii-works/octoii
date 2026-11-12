/**
 * The marks lines of work lead with, one a line, in the slot each line gives one: a call's, its kind of work's icon on
 * a round — a tool the page has no icon for, its name's first letter — red where it went wrong.
 */
import type { ReactNode } from "react";
import { Code, FilePen, FileText, Globe, type LucideIcon, MessageCircleQuestion, MessagesSquare, Search, Split, SquareTerminal } from "lucide-react";
import type { WorkKind } from "../../core/tools.ts";
import { cn } from "../ui/cn.ts";

/** Each kind of work's icon; a tool of no kind the page knows is marked by its name. */
const ICONS: Readonly<Record<Exclude<WorkKind, "other">, LucideIcon>> = {
	read: FileText,
	edit: FilePen,
	search: Search,
	command: SquareTerminal,
	script: Code,
	browse: Globe,
	ask: MessageCircleQuestion,
	delegate: Split,
	thread: MessagesSquare,
};

/** A line's mark: `children` on a round, red where what it marks went wrong. */
export function Mark(props: { readonly children: ReactNode; readonly troubled?: boolean; readonly className?: string }): ReactNode {
	return <span className={cn("tool-mark", props.troubled === true && "text-error-foreground", props.className)}>{props.children}</span>;
}

/** A call's mark: its kind of work's icon, or its tool's first letter, in capitals. */
export function CallMark(props: {
	readonly call: { readonly work: WorkKind; readonly tool: string; readonly state: string; readonly trouble: string | undefined };
}): ReactNode {
	const { call } = props;
	const troubled = call.state === "failed" || call.trouble !== undefined;
	if (call.work === "other") {
		return (
			<Mark troubled={troubled} className="text-2xs font-semibold">
				{call.tool.charAt(0).toUpperCase()}
			</Mark>
		);
	}
	const Icon = ICONS[call.work];
	return (
		<Mark troubled={troubled}>
			<Icon />
		</Mark>
	);
}
