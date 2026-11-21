import type { ReactNode } from "react";
import { ChevronDown, ChevronUp, X } from "lucide-react";
import type { useBrowserFind } from "../../components/effects/browser-find.ts";
import { IconButton } from "../../components/ui/button.tsx";
import { Field } from "../../components/ui/input.tsx";

export function FindBar(props: { readonly find: ReturnType<typeof useBrowserFind>; readonly onClose: () => void }): ReactNode {
	const { search, field, edit, move } = props.find;
	if (search === undefined) return null;
	const matches = search.match?.matches ?? 0;
	const count = search.match === undefined ? "" : matches === 0 ? "No matches" : `${String(search.match.activeMatchOrdinal)} of ${String(matches)}`;
	return (
		<form
			role="search"
			aria-label="Find in page"
			className="flex shrink-0 items-center gap-1 border-b border-border px-2 py-1"
			onSubmit={(event) => {
				event.preventDefault();
				move("forward");
			}}
			onKeyDown={(event) => {
				if (event.nativeEvent.isComposing) return;
				if (event.key === "Escape") {
					event.preventDefault();
					event.stopPropagation();
					props.onClose();
				}
				if (event.key === "Enter" && event.shiftKey && event.target === field.current) {
					event.preventDefault();
					move("backward");
				}
			}}
		>
			<Field
				ref={field}
				label="Find in page"
				placeholder="Find in page"
				type="search"
				size="sm"
				autoFocus
				value={search.query}
				onChange={(event) => edit(event.currentTarget.value)}
			/>
			<span role="status" className="shrink-0 px-1 text-xs text-muted-foreground tabular-nums">
				{count}
			</span>
			<IconButton label="Previous match" disabled={matches === 0} onClick={() => move("backward")}>
				<ChevronUp className="size-3.5" />
			</IconButton>
			<IconButton label="Next match" disabled={matches === 0} onClick={() => move("forward")}>
				<ChevronDown className="size-3.5" />
			</IconButton>
			<IconButton label="Close find" onClick={props.onClose}>
				<X className="size-3.5" />
			</IconButton>
		</form>
	);
}
