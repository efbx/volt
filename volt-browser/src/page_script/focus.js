const FOCUSABLE = 'a[href],button,input,select,textarea,summary,iframe,[tabindex],[contenteditable=""],[contenteditable="true"]';

function active(root) {
  let el = root.activeElement;
  while (el && el.shadowRoot && el.shadowRoot.activeElement) el = el.shadowRoot.activeElement;
  return el;
}

function emit(el, type, bubbles) {
  const owner = view(el);
  el.dispatchEvent(new owner.FocusEvent(type, { bubbles: bubbles, composed: true, view: owner }));
}

function focus(el) {
  const owner = el.ownerDocument;
  const before = active(owner);
  if (before === el) return;
  let heard = false;
  let left = false;
  const hear = function () {
    heard = true;
  };
  const leave = function () {
    left = true;
  };
  el.addEventListener("focus", hear, true);
  if (before) before.addEventListener("blur", leave, true);
  el.focus({ preventScroll: true });
  el.removeEventListener("focus", hear, true);
  if (before) before.removeEventListener("blur", leave, true);
  if (active(owner) !== el) return;
  if (before && before !== owner.body && !left) {
    emit(before, "blur", false);
    emit(before, "focusout", true);
  }
  if (!heard) {
    emit(el, "focus", false);
    emit(el, "focusin", true);
  }
}

function blur(el) {
  if (active(el.ownerDocument) !== el) return;
  let left = false;
  const leave = function () {
    left = true;
  };
  el.addEventListener("blur", leave, true);
  el.blur();
  el.removeEventListener("blur", leave, true);
  if (!left) {
    emit(el, "blur", false);
    emit(el, "focusout", true);
  }
}

function engage(el) {
  const target = el.closest(FOCUSABLE);
  if (target && enabled(target)) return focus(target);
  const current = active(el.ownerDocument);
  if (current && current !== el.ownerDocument.body) blur(current);
}
