/**
 * Draws Octoii's mark and logo into `brand/` at the repository's root. The mark is an octopus in one solid shape: a round
 * head whose sides flare out into the tips of its arms, the two middle tips short bumps between them, the eyes cut out.
 * The logo is the mark with "Octoii" beside it in Inter Bold, outlined, its capitals centred on the mark. The app's
 * icons are the mark, white on #141414, filling 85% of their width, but for the one a mask may cut to a circle; the
 * favicon is the mark on its tile of 16.
 *
 *   bun scripts/brand.ts
 */
import { mkdir, writeFile } from "node:fs/promises";
import { Resvg } from "@resvg/resvg-js";
import { openSync, type Font } from "fontkit";

const BRAND = new URL("../../../brand/", import.meta.url).pathname;

/** The ink and the ground of the icons. */
const INK = "#ffffff";
const GROUND = "#141414";

type Point = readonly [number, number];

/** One step of an outline: a move to a point, a cubic curve through two controls to a point, or a close. */
type Step = { readonly move: Point } | { readonly curve: readonly [Point, Point, Point] } | "close";

/** How far a cubic's controls stand from its ends to draw a quarter of a circle of radius 1. */
const QUARTER = 0.5523;

/** A circle at `cx`, `cy` of radius `r`. */
function circle(cx: number, cy: number, r: number): Step[] {
	const k = r * QUARTER;
	return [
		{ move: [cx + r, cy] },
		{
			curve: [
				[cx + r, cy + k],
				[cx + k, cy + r],
				[cx, cy + r],
			],
		},
		{
			curve: [
				[cx - k, cy + r],
				[cx - r, cy + k],
				[cx - r, cy],
			],
		},
		{
			curve: [
				[cx - r, cy - k],
				[cx - k, cy - r],
				[cx, cy - r],
			],
		},
		{
			curve: [
				[cx + k, cy - r],
				[cx + r, cy - k],
				[cx + r, cy],
			],
		},
		"close",
	];
}

/** The head's radius on the mark's square of 100. */
const HEAD = 28;

/** The mark on a square of 100: the head, a half circle on its sides, flaring into the arms' tips; and the eyes, cut out
 * by the even-odd rule. */
const MARK: readonly Step[] = [
	{ move: [14, 84] },
	{
		curve: [
			[22, 78],
			[22, 66],
			[22, 50],
		],
	},
	{
		curve: [
			[22, 50 - HEAD * QUARTER],
			[50 - HEAD * QUARTER, 22],
			[50, 22],
		],
	},
	{
		curve: [
			[50 + HEAD * QUARTER, 22],
			[78, 50 - HEAD * QUARTER],
			[78, 50],
		],
	},
	{
		curve: [
			[78, 66],
			[78, 78],
			[86, 84],
		],
	},
	{
		curve: [
			[78, 90],
			[68, 86],
			[64, 78],
		],
	},
	{
		curve: [
			[60, 86],
			[54, 88],
			[50, 88],
		],
	},
	{
		curve: [
			[46, 88],
			[40, 86],
			[36, 78],
		],
	},
	{
		curve: [
			[32, 86],
			[22, 90],
			[14, 84],
		],
	},
	"close",
	...circle(41, 52, 5),
	...circle(59, 52, 5),
];

/** `n` to two places, without trailing zeros. */
const num = (n: number): string => String(Math.round(n * 100) / 100);

/** Every number in a path to two places. */
const rounded = (d: string): string => d.replaceAll(/-?\d+(\.\d+)?(e-?\d+)?/g, (n) => num(Number(n)));

/** `steps` as a path, scaled by `k` about the origin and then moved by `dx`, `dy`. */
function outline(steps: readonly Step[], k: number, [dx, dy]: Point): string {
	const at = ([x, y]: Point): string => `${num(x * k + dx)} ${num(y * k + dy)}`;
	return steps.map((step) => (step === "close" ? "Z" : "move" in step ? `M${at(step.move)}` : `C${step.curve.map(at).join(" ")}`)).join("");
}

/** A figure's box: its left, top, width and height. */
type Box = { readonly x: number; readonly y: number; readonly w: number; readonly h: number };

/** The box the path `d` fills. */
function bounds(d: string): Box {
	const box = new Resvg(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000"><path fill-rule="evenodd" d="${d}"/></svg>`).getBBox();
	if (box === undefined) throw new Error("The figure draws nothing.");
	return { x: box.x, y: box.y, w: box.width, h: box.height };
}

/** The mark's box on its square of 100. */
const MARK_BOX = bounds(outline(MARK, 1, [0, 0]));

/** The mark scaled by `k`, its box's corner at `corner`. */
const markAt = (k: number, [x, y]: Point): string => outline(MARK, k, [x - MARK_BOX.x * k, y - MARK_BOX.y * k]);

/** The height of the logo and of the mark alone: their pixels at their smaller size. */
const HEIGHT = 22;

/** The mark's width at `HEIGHT`. */
const MARK_WIDTH = (MARK_BOX.w * HEIGHT) / MARK_BOX.h;

/** The logo's type: Inter Bold, its capitals 80% of the mark's height, set a little tight, a gap of 40% of a capital
 * after the mark. */
const TYPE = { file: "@fontsource/inter/files/inter-latin-700-normal.woff", caps: 0.8, tracking: -0.02, gap: 0.4 };

/** Inter Bold. */
function inter(): Font {
	const font = openSync(new URL(import.meta.resolve(TYPE.file)).pathname);
	if (!("layout" in font)) throw new Error(`${TYPE.file} is a collection, not one font.`);
	return font;
}

/** The logo: the mark `HEIGHT` high at the origin, "Octoii" beside it, outlined; its width and path. */
function logo(): { w: number; d: string } {
	const font = inter();
	const k = (HEIGHT * TYPE.caps) / font.capHeight;
	const baseline = (HEIGHT + font.capHeight * k) / 2;
	const { glyphs, positions } = font.layout("Octoii");
	let x = MARK_WIDTH + TYPE.gap * font.capHeight * k - (glyphs[0]?.bbox.minX ?? 0) * k;
	const letters = glyphs.map((glyph, index) => {
		const d = glyph.path.scale(k, -k).translate(x, baseline).toSVG();
		x += ((positions[index]?.xAdvance ?? 0) + TYPE.tracking * font.unitsPerEm) * k;
		return rounded(d);
	});
	const d = [markAt(HEIGHT / MARK_BOX.h, [0, 0]), ...letters].join("");
	const box = bounds(d);
	return { w: box.x + box.w, d };
}

/** The path `d`, `w` by `HEIGHT`, in the colour of the text around it. */
const figureSvg = (w: number, d: string): string =>
	`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${num(w)} ${HEIGHT}"><path fill="currentColor" fill-rule="evenodd" d="${d}"/></svg>\n`;

/** The mark white on a square of #141414 of `size`, its corners rounded by `radius`, the mark `share` of its width, centred. */
function tile(size: number, share: number, radius = 0): string {
	const k = (size * share) / MARK_BOX.w;
	const corner: Point = [(size - MARK_BOX.w * k) / 2, (size - MARK_BOX.h * k) / 2];
	return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}"><rect width="${size}" height="${size}" rx="${radius}" fill="${GROUND}"/><path fill="${INK}" fill-rule="evenodd" d="${markAt(k, corner)}"/></svg>\n`;
}

/** A tile drawn as a PNG. */
const png = (svg: string): Buffer => new Resvg(svg).render().asPng();

/** How much of an icon's width the mark fills where only the icon's corners are cut. */
const FILL = 0.85;

/** How much of an icon's width the mark fills where a mask may cut it to a circle of radius 40%: its box's corners on
 * that circle. */
const MASKABLE = (0.8 * MARK_BOX.w) / Math.hypot(MARK_BOX.w, MARK_BOX.h);

if (import.meta.main) {
	await mkdir(BRAND, { recursive: true });
	const { w, d } = logo();
	const files: [string, string | Buffer][] = [
		["mark.svg", figureSvg(MARK_WIDTH, markAt(HEIGHT / MARK_BOX.h, [0, 0]))],
		["logo.svg", figureSvg(w, d)],
		["favicon.svg", tile(16, 0.82, 3)],
		["icon-512.png", png(tile(512, FILL))],
		["icon-192.png", png(tile(192, FILL))],
		["apple-touch-icon.png", png(tile(180, FILL))],
		["icon-maskable-512.png", png(tile(512, MASKABLE))],
	];
	for (const [name, data] of files) await writeFile(`${BRAND}${name}`, data);
	console.log(`Drew ${files.map(([name]) => name).join(", ")} into ${BRAND}`);
}
