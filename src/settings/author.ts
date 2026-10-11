import { visibleLinkCount } from "../shared/link-overflow";
import type { Skin } from "../shared/types";

export function initializeAuthor(openLink: (label: string) => void) {
  const row = document.querySelector<HTMLElement>("#skin-author")!;
  const name = document.querySelector<HTMLElement>("#skin-author-name")!;
  const links = document.querySelector<HTMLElement>("#skin-author-links")!;
  const more = document.querySelector<HTMLDetailsElement>("#skin-author-more")!;
  const summary = more.querySelector("summary")!;
  const menu = document.querySelector<HTMLElement>("#skin-author-menu")!;
  let anchors: HTMLAnchorElement[] = [];
  let signature = "";
  let scheduled = 0;

  function layout() {
    scheduled = 0;
    const rowWidth = row.getBoundingClientRect().width;
    if (!rowWidth) return;
    if (!anchors.length) {
      name.style.maxWidth = "100%";
      return;
    }
    // Measure real text after font loading, without estimating character lengths.
    for (const anchor of anchors) links.insertBefore(anchor, more);
    more.hidden = true;
    const widths = anchors.map((anchor) => anchor.getBoundingClientRect().width);
    const gap = Number.parseFloat(getComputedStyle(links).columnGap) || 0;
    // Reserve only one real link (or More), not a fixed percentage of the row.
    more.hidden = false;
    more.style.maxWidth = "none";
    const moreWidth = more.getBoundingClientRect().width;
    more.style.maxWidth = "";
    more.hidden = true;
    const separation = Number.parseFloat(getComputedStyle(links).marginLeft) || 0;
    const reserved = Math.min(widths[0], moreWidth);
    name.style.maxWidth = `${Math.max(0, rowWidth - separation - reserved)}px`;
    const available = links.getBoundingClientRect().width;
    // Test the real all-links layout before introducing More into the flex row.
    if (visibleLinkCount(widths, available, 0, gap) === anchors.length) {
      more.open = false;
      return;
    }
    more.hidden = false;
    const count = visibleLinkCount(widths, available, moreWidth, gap);
    anchors.slice(count).forEach((anchor) => menu.append(anchor));
    more.hidden = count === anchors.length;
    if (more.hidden) more.open = false;
  }

  function schedule() {
    if (!scheduled) scheduled = requestAnimationFrame(layout);
  }
  const observer = new ResizeObserver(schedule);
  observer.observe(row);
  observer.observe(links);
  void document.fonts.ready.then(schedule);
  document.fonts.addEventListener("loadingdone", schedule);
  document.addEventListener("pointerdown", (event) => {
    if (!more.contains(event.target as Node)) more.open = false;
  });
  more.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      more.open = false;
      summary.focus();
    }
  });
  window.addEventListener("beforeunload", () => {
    observer.disconnect();
    cancelAnimationFrame(scheduled);
    document.fonts.removeEventListener("loadingdone", schedule);
  });

  return (author: Skin["author"]) => {
    const next = JSON.stringify(author);
    if (signature === next) return;
    signature = next;
    row.classList.toggle("empty", !author);
    row.setAttribute("aria-hidden", String(!author));
    name.textContent = author ? `By ${author.name}` : "";
    name.title = author?.name ?? "";
    anchors.forEach((anchor) => anchor.remove());
    anchors = [];
    more.open = false;
    more.hidden = true;
    for (const [label, url] of Object.entries(author?.links ?? {})) {
      const link = document.createElement("a");
      link.textContent = label;
      link.href = url;
      link.title = url;
      link.addEventListener("click", (event) => {
        event.preventDefault();
        more.open = false;
        openLink(label);
      });
      anchors.push(link);
      links.insertBefore(link, more);
    }
    schedule();
  };
}