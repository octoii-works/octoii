/**
 * What each extension gives the page, listed for the app: the views of the things messages name — a thread by the
 * thread extension's, a page by the browser's, a terminal by the terminals' — and how a call of each extension's tools
 * shows, registered as the app loads this.
 */
import type { Contributions } from "../components/state/contributions.ts";
import { registerTools } from "../core/tools.ts";
import { BROWSER_TOOLS } from "./browser/tool-view.ts";
import { DELEGATE_TOOLS } from "./delegate/tool-view.ts";
import { QUESTION_TOOLS } from "./question/tool-view.ts";
import { TERMINAL_TOOLS } from "./terminals/tool-view.ts";
import { THREAD_TOOLS } from "./thread/tool-view.ts";
import { PAGE_VIEW } from "./browser/page-thing.tsx";
import { TERMINAL_VIEW } from "./terminals/terminal-thing.tsx";
import { THREAD_VIEW } from "./thread/thread-thing.tsx";

/** What the extensions give. */
export const CONTRIBUTIONS: Contributions = { things: { thread: THREAD_VIEW, page: PAGE_VIEW, terminal: TERMINAL_VIEW } };

registerTools({ ...BROWSER_TOOLS, ...DELEGATE_TOOLS, ...QUESTION_TOOLS, ...TERMINAL_TOOLS, ...THREAD_TOOLS });
