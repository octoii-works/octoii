/** File controls float over the preview on hover or focus; touch screens keep their entry points visible. */
import type { ReactNode } from "react";
import { Ellipsis } from "lucide-react";
import { IconButton } from "../ui/button.tsx";
import { Menu, MenuPopup, MenuTrigger } from "../ui/menu.tsx";

export function FileActions(props: { readonly children: ReactNode }): ReactNode {
	return (
		<div className="pointer-events-none absolute right-2 top-2 z-10 flex items-center gap-1 rounded-lg border border-border bg-background p-1 opacity-0 shadow-floating transition-opacity group-hover/file:pointer-events-auto group-hover/file:opacity-100 focus-within:pointer-events-auto focus-within:opacity-100 [&:has([data-popup-open])]:pointer-events-auto [&:has([data-popup-open])]:opacity-100 [@media(hover:none)]:pointer-events-auto [@media(hover:none)]:opacity-100 [@media(pointer:coarse)]:pointer-events-auto [@media(pointer:coarse)]:opacity-100">
			{props.children}
		</div>
	);
}

/** Secondary file actions have a visible, keyboard-accessible entry point without occupying a separate row. */
export function FileMore(props: { readonly name: string; readonly children: ReactNode }): ReactNode {
	return (
		<Menu>
			<MenuTrigger render={<IconButton label={`More actions for ${props.name}`} />}>
				<Ellipsis />
			</MenuTrigger>
			<MenuPopup align="end">{props.children}</MenuPopup>
		</Menu>
	);
}
