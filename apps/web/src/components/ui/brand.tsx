/**
 * Octoii's logo, as `scripts/brand.ts` draws it into `brand/`: the mark, an octopus, and "Octoii" beside it, in the ink
 * of the words around it.
 */
import type { ReactNode } from "react";
import logo from "../../../../../brand/logo.svg";

/** How large the logo stands: 22 pixels high, the mark's height, or twice that. */
const SIZES = { sm: "h-[22px]", lg: "h-[44px]" } as const;

/** The logo, its width from its height. */
export function Logo(props: { readonly size?: keyof typeof SIZES }): ReactNode {
	return (
		<span
			role="img"
			aria-label="Octoii"
			style={{ "--mark": `url("${logo}")` }}
			className={`mark-ink aspect-[95.63/22] shrink-0 ${SIZES[props.size ?? "sm"]}`}
		/>
	);
}
