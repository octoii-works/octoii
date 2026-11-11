/**
 * Octoii's own marks, as `scripts/brand.ts` draws them into `brand/`, in the ink of the words beside them and at one
 * pixel a cell, so every edge falls on a whole pixel: the wordmark, and the mark, Oii, on its grid of 64.
 */
import type { ReactNode } from "react";
import mark from "../../../../../brand/mark-64.svg";
import wordmark from "../../../../../brand/wordmark.svg";

/** The wordmark, "Octoii", its capitals 18 pixels high. */
export function Wordmark(): ReactNode {
	return <span role="img" aria-label="Octoii" style={{ "--mark": `url("${wordmark}")` }} className="mark-ink h-[18px] w-[61px] shrink-0" />;
}

/** The mark, Oii, 37 pixels high. */
export function Mark(): ReactNode {
	return <span role="img" aria-label="Octoii" style={{ "--mark": `url("${mark}")` }} className="mark-ink h-[37px] w-[53px] shrink-0" />;
}
