/**
 * Octoii's wordmark, as `scripts/brand.ts` draws it into `brand/`, in the ink of the words beside it and at a whole
 * number of pixels a cell, so every edge falls on a whole pixel.
 */
import type { ReactNode } from "react";
import wordmark from "../../../../../brand/wordmark.svg";

/** How large the wordmark stands: one pixel a cell, its capitals 18 pixels high, or two. */
const SIZES = { sm: "h-[18px] w-[61px]", lg: "h-[36px] w-[122px]" } as const;

/** The wordmark, "Octoii". */
export function Wordmark(props: { readonly size?: keyof typeof SIZES }): ReactNode {
	return <span role="img" aria-label="Octoii" style={{ "--mark": `url("${wordmark}")` }} className={`mark-ink shrink-0 ${SIZES[props.size ?? "sm"]}`} />;
}
