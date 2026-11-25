const TYPED = { text: true, search: true, url: true, tel: true, password: true, email: true, number: true };
const VALUED = { date: true, time: true, "datetime-local": true, month: true, week: true, color: true, range: true };

function visible(el) {
  if (el.nodeType !== 1) return false;
  const rect = el.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0) return false;
  return view(el).getComputedStyle(el).visibility !== "hidden";
}

function box(el) {
  const rect = el.getBoundingClientRect();
  return { x: rect.left, y: rect.top, width: rect.width, height: rect.height };
}

function enabled(el) {
  return !el.matches(":disabled") && !el.closest('[aria-disabled="true"]');
}

function editing(el) {
  if (el.localName === "textarea") return "text";
  if (el.localName === "input") {
    if (TYPED[el.type]) return "text";
    return VALUED[el.type] ? "value" : null;
  }
  return el.isContentEditable ? "rich" : null;
}

function locked(el) {
  return el.readOnly === true || el.getAttribute("aria-readonly") === "true";
}

function editable(el) {
  return !!editing(el) && enabled(el) && !locked(el);
}

function same(a, b) {
  return Math.abs(a.x - b.x) < 0.5 && Math.abs(a.y - b.y) < 0.5 &&
    Math.abs(a.width - b.width) < 0.5 && Math.abs(a.height - b.height) < 0.5;
}

function aim(rect, offset) {
  if (offset) return { x: rect.x + offset.x, y: rect.y + offset.y };
  return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
}

function struck(el, point) {
  let target = el.ownerDocument.elementFromPoint(point.x, point.y);
  while (target && target.shadowRoot) {
    const inner = target.shadowRoot.elementFromPoint(point.x, point.y);
    if (!inner || inner === target) break;
    target = inner;
  }
  return target;
}

function reachable(el, point) {
  const target = struck(el, point);
  for (let node = target; node; node = node.parentNode || node.host) {
    if (node === el) return true;
  }
  const label = target && target.closest ? target.closest("label") : null;
  return !!label && label.control === el;
}

function reveal(el) {
  if (el.scrollIntoViewIfNeeded) el.scrollIntoViewIfNeeded(true);
  else el.scrollIntoView({ block: "center", inline: "center" });
}

function ready(el, needs, offset) {
  if (needs.visible && !visible(el)) halt(wait("hidden"));
  if (needs.enabled && !enabled(el)) halt(wait("disabled"));
  if (needs.editable) {
    if (!editing(el)) halt(fail("wrong", "it is not a field that takes text"));
    if (locked(el)) halt(wait("read_only"));
  }
  if (needs.still || needs.reachable) reveal(el);
  const rect = box(el);
  if (needs.still && !(command.rect && same(command.rect, rect))) halt(wait("moving", rect));
  const point = aim(rect, offset);
  if (needs.reachable && !reachable(el, point)) halt(wait("covered"));
  return point;
}
