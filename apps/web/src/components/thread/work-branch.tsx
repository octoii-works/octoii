/** The details beneath a line of work, indented from its mark and joined to it by a guide. */
import type { ReactNode } from "react";
import { cn } from "../ui/cn.ts";

export function WorkBranch(props: { readonly children: ReactNode; readonly className?: string }): ReactNode {
	return (
		<div data-slot="work-branch" className={cn("ms-2.5 border-s border-border ps-4", props.className)}>
			{props.children}
		</div>
	);
}
