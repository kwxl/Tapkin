export const field = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T;

export function updateRangeProgress(range: HTMLInputElement) {
  const min = Number(range.min) || 0;
  const max = Number(range.max) || 100;
  const value = Number(range.value);
  const progress = max > min ? ((value - min) / (max - min)) * 100 : 0;
  range.style.setProperty("--range-progress", `${progress}%`);
}

