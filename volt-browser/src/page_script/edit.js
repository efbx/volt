function assign(el, value) {
  const owner = view(el);
  const kind = el.localName === "textarea" ? owner.HTMLTextAreaElement : owner.HTMLInputElement;
  Object.getOwnPropertyDescriptor(kind.prototype, "value").set.call(el, value);
}

function announce(el, kind, data) {
  const owner = view(el);
  el.dispatchEvent(new owner.InputEvent("input", { bubbles: true, composed: true, inputType: kind, data: data }));
}

function changed(el) {
  el.dispatchEvent(new (view(el).Event)("change", { bubbles: true }));
}

function ending(el) {
  const range = el.ownerDocument.createRange();
  range.selectNodeContents(el);
  range.collapse(false);
  const selection = view(el).getSelection();
  selection.removeAllRanges();
  selection.addRange(range);
}

function fill(el, text) {
  ready(el, { visible: true, enabled: true, editable: true });
  const kind = text ? "insertText" : "deleteContentBackward";
  focus(el);
  if (editing(el) === "rich") {
    el.textContent = text;
    ending(el);
    return announce(el, kind, text || null);
  }
  assign(el, text);
  announce(el, kind, text || null);
  changed(el);
}

function span(el) {
  let start = null;
  let end = null;
  try {
    start = el.selectionStart;
    end = el.selectionEnd;
  } catch (error) {
    start = null;
  }
  if (typeof start !== "number" || typeof end !== "number") {
    return { start: el.value.length, end: el.value.length };
  }
  return { start: start, end: end };
}

function place(el, at) {
  try {
    el.setSelectionRange(at, at);
  } catch (error) {
    return false;
  }
  return true;
}

function caret(el) {
  const selection = view(el).getSelection();
  if (!selection.rangeCount || !el.contains(selection.anchorNode)) ending(el);
}

function insert(el, text) {
  const kind = editing(el);
  if (!kind || kind === "value" || locked(el) || !enabled(el)) return;
  if (kind === "rich") {
    caret(el);
    if (el.ownerDocument.execCommand("insertText", false, text)) return;
    el.appendChild(el.ownerDocument.createTextNode(text));
    return announce(el, "insertText", text);
  }
  const at = span(el);
  const next = el.value.slice(0, at.start) + text + el.value.slice(at.end);
  if (el.maxLength >= 0 && next.length > el.maxLength) return;
  const owner = view(el);
  const before = new owner.InputEvent("beforeinput", {
    bubbles: true, cancelable: true, composed: true, inputType: "insertText", data: text
  });
  if (!el.dispatchEvent(before)) return;
  assign(el, next);
  place(el, at.start + text.length);
  announce(el, "insertText", text);
}

function erase(el, forward) {
  const kind = editing(el);
  if (!kind || kind === "value" || locked(el) || !enabled(el)) return;
  if (kind === "rich") {
    caret(el);
    el.ownerDocument.execCommand(forward ? "forwardDelete" : "delete");
    return;
  }
  const at = span(el);
  if (at.start === at.end) {
    if (forward) at.end = Math.min(el.value.length, at.end + 1);
    else at.start = Math.max(0, at.start - 1);
  }
  if (at.start === at.end) return;
  assign(el, el.value.slice(0, at.start) + el.value.slice(at.end));
  place(el, at.start);
  announce(el, forward ? "deleteContentForward" : "deleteContentBackward", null);
}

function move(el, key) {
  if (editing(el) !== "text") return;
  const at = span(el);
  const length = el.value.length;
  if (key === "Home") place(el, 0);
  else if (key === "End") place(el, length);
  else if (key === "ArrowLeft") place(el, Math.max(0, at.start - 1));
  else place(el, Math.min(length, at.end + 1));
}

function mark(el) {
  if (el.select) return el.select();
  const range = el.ownerDocument.createRange();
  range.selectNodeContents(el);
  const selection = view(el).getSelection();
  selection.removeAllRanges();
  selection.addRange(range);
}
