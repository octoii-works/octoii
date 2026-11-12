/**
 * The things the work of a thread made that stand on their own — a sub-thread, a terminal — as lines of the work: a
 * mark with a dot of how the thing stands, in the slot every line gives one; its name, and under it, quieter, what it
 * does or what came of it; its time; and a way into it. How it stands is its dot's alone — going on, the dot breathes and
 * its time counts; screen readers are told in words — and the lines of the work around it keep their edges.
 */
import type { ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import type { Standing } from "../../core/sub-threads.ts";
import { cn } from "../ui/cn.ts";
import { FoldChevron } from "../ui/collapsible.tsx";
import { Mark } from "./marks.tsx";

/** Each way a thing stands: its dot's colour, and the words screen readers are told. */
const STANDINGS: Readonly<Record<Standing, { readonly dot: string; readonly words: string }>> = {
	running: { dot: "bg-info animate-breathe motion-reduce:animate-none", words: "working" },
	completed: { dot: "bg-success", words: "done" },
	failed: { dot: "bg-error", words: "failed" },
	stopped: { dot: "bg-faint-foreground", words: "stopped" },
};

/** The dot of how a thing stands, on the corner of its mark or its icon — wherever a thing's standing is drawn. */
export function StandingDot(props: { readonly standing: Standing }): ReactNode {
	return <span className={cn("absolute -right-0.5 -bottom-0.5 size-1.5 rounded-full ring-2 ring-background", STANDINGS[props.standing].dot)} />;
}

/** A thing's mark: `children` on a round, with a dot of how it stands. */
export function EntityMark(props: { readonly standing: Standing; readonly children: ReactNode }): ReactNode {
	return (
		<Mark className="relative">
			{props.children}
			<StandingDot standing={props.standing} />
		</Mark>
	);
}

/** How an entity's line opens: to the thing, elsewhere; in place, folded under it; or not at all. */
export type Opens = "elsewhere" | "in-place" | undefined;

/** What an entity's line shows: its mark, its name, what it does or what came of it, its time, and how it opens. */
export type EntityProps = {
	readonly mark: ReactNode;
	readonly standing: Standing;
	readonly name: ReactNode;
	readonly detail?: ReactNode;
	readonly time?: ReactNode;
	readonly opens: Opens;
};

/** An entity's line's content, the same whether the line is a link, a fold's trigger, or neither. */
export function EntityContent(props: EntityProps): ReactNode {
	return (
		<>
			{props.mark}
			<span className="flex min-w-0 flex-1 flex-col py-1">
				<span className="truncate text-foreground">{props.name}</span>
				{props.detail === undefined ? null : <span className="truncate text-xs text-muted-foreground">{props.detail}</span>}
			</span>
			<span className="sr-only">{STANDINGS[props.standing].words}</span>
			{props.time === undefined ? null : <span className="shrink-0 text-xs text-muted-foreground tabular-nums">{props.time}</span>}
			{props.opens === "elsewhere" ? (
				<ChevronRight className="size-3.5 shrink-0 text-faint-foreground opacity-0 transition-opacity group-hover/entity:opacity-100 group-focus-visible/entity:opacity-100 pointer-coarse:opacity-100" />
			) : props.opens === "in-place" ? (
				<FoldChevron className="quiet-chevron" />
			) : null}
		</>
	);
}
