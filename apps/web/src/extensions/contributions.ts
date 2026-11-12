/**
 * What each extension gives the page, listed for the app: the views of the things messages name — a thread by the
 * thread extension's, a page by the browser's, a terminal by the terminals'.
 */
import type { Contributions } from "../components/state/contributions.ts";
import { PAGE_VIEW } from "./browser/page-thing.tsx";
import { TERMINAL_VIEW } from "./terminals/terminal-thing.tsx";
import { THREAD_VIEW } from "./thread/thread-thing.tsx";

/** What the extensions give. */
export const CONTRIBUTIONS: Contributions = { things: { thread: THREAD_VIEW, page: PAGE_VIEW, terminal: TERMINAL_VIEW } };
