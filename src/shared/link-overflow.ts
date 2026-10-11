/** Keep an ordered prefix visible, reserving room for More only when needed. */
export function visibleLinkCount(widths: number[], available: number, moreWidth: number, gap: number): number {
  const total = widths.reduce((sum, width) => sum + width, 0) + Math.max(0, widths.length - 1) * gap;
  // Allow only subpixel layout noise, not a whole extra pixel of overflow.
  const tolerance = 0.01;
  if (total <= available + tolerance) return widths.length;
  let used = moreWidth;
  let count = 0;
  for (const width of widths) {
    if (used + gap + width > available + tolerance) break;
    used += gap + width;
    count += 1;
  }
  return count;
}