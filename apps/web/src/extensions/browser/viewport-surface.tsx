import { type ReactNode, useCallback } from "react";
import type { ViewportPreview } from "../../components/effects/browser-layout.ts";
import { placeDesktopPage, type DesktopPage } from "./desktop.ts";

/** The same surface stays mounted across layout changes, retaining its viewer and its control. */
export function ViewportSurface({
	mountRoom,
	preview,
	desktop,
	hidden,
	children,
}: {
	readonly mountRoom: (element: HTMLDivElement | null) => (() => void) | undefined;
	readonly preview: ViewportPreview | undefined;
	readonly desktop: DesktopPage | undefined;
	readonly hidden: boolean;
	readonly children: ReactNode;
}): ReactNode {
	const width = preview === undefined ? "100%" : preview.size.width * preview.scale;
	const height = preview === undefined ? "100%" : preview.size.height * preview.scale;
	const across = preview === undefined ? 1 : (preview.size.width * preview.scale) / preview.view.width;
	const down = preview === undefined ? 1 : (preview.size.height * preview.scale) / preview.view.height;
	const transform = `scale(${String(across)}, ${String(down)})`;
	const slot = useCallback(
		(element: HTMLDivElement | null) => (desktop && element ? placeDesktopPage(desktop, element, { across, down, transform }) : undefined),
		[desktop, across, down, transform],
	);
	return (
		<div ref={mountRoom} data-browser-viewport="" hidden={hidden} className="relative min-h-0 flex-1 overflow-auto bg-muted">
			<div
				className="grid min-h-full min-w-full place-items-center"
				style={
					preview === undefined
						? { width: "100%", height: "100%" }
						: {
								width: preview.size.width * preview.scale + 2 * preview.margin,
								height: preview.size.height * preview.scale + 2 * preview.margin,
							}
				}
			>
				<div className="relative overflow-hidden bg-white" style={{ width, height }}>
					<div
						ref={slot}
						className="absolute top-0 left-0 origin-top-left overflow-hidden"
						style={preview === undefined ? { width: "100%", height: "100%" } : { width: preview.view.width, height: preview.view.height, transform }}
					>
						{desktop === undefined ? children : desktop.render(children)}
					</div>
				</div>
			</div>
		</div>
	);
}
