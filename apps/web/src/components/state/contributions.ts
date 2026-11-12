/**
 * What the extensions give the parts drawn deep in the page — a thing a message names, drawn by the view of the
 * extension of its kind — read through the page, so no part of the page imports an extension: the app gives them, as
 * each extension lists what it gives.
 */
import { createContext } from "react";
import type { Kind } from "../../core/things.ts";
import type { ThingView } from "../thread/thing-look.tsx";

/** What the extensions give: the view of each kind of thing a message names. */
export type Contributions = { readonly things: Partial<Record<Kind, ThingView>> };

/** What the extensions give the page; nothing where the app gives nothing. */
export const ContributionsContext = createContext<Contributions>({ things: {} });
