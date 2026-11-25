const KEY_NUMBERS = {
  Enter: 13, Tab: 9, Backspace: 8, Delete: 46, Escape: 27, " ": 32,
  ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40,
  Home: 36, End: 35, PageUp: 33, PageDown: 34, Insert: 45,
  Shift: 16, Control: 17, Alt: 18, Meta: 91
};
const PRESSABLE = 'button,summary,input[type="checkbox"],input[type="radio"],input[type="button"],input[type="submit"],input[type="reset"],[role="button"]';
const ACTIVATED = 'a[href],button,summary,input[type="button"],input[type="submit"],input[type="reset"],[role="button"]';
const SUBMITTERS = 'button:not([type]):not(:disabled),button[type="submit"]:not(:disabled),input[type="submit"]:not(:disabled),input[type="image"]:not(:disabled)';

function enter(el) {
  if (el.localName === "textarea") return insert(el, "\n");
  if (editing(el) === "rich") {
    caret(el);
    if (!el.ownerDocument.execCommand("insertParagraph")) insert(el, "\n");
    return;
  }
  if (el.matches(ACTIVATED)) return el.click();
  if (el.localName !== "input" || !el.form) return;
  const button = el.form.querySelector(SUBMITTERS);
  if (button) button.click();
  else submit(el);
}

function tab(el, backwards) {
  const stops = query(el.ownerDocument, FOCUSABLE).filter(function (node) {
    return node.tabIndex >= 0 && enabled(node) && visible(node);
  });
  const rank = function (node) {
    return node.tabIndex > 0 ? node.tabIndex : 32768;
  };
  stops.sort(function (a, b) {
    return rank(a) - rank(b);
  });
  const index = stops.indexOf(el);
  let next = stops[index + (backwards ? -1 : 1)];
  if (index < 0) next = backwards ? stops[stops.length - 1] : stops[0];
  if (next) focus(next);
}

function keyed(key) {
  let code = "";
  let number = KEY_NUMBERS[key] || 0;
  if (/^[a-z]$/i.test(key)) {
    code = "Key" + key.toUpperCase();
    number = key.toUpperCase().charCodeAt(0);
  } else if (/^[0-9]$/.test(key)) {
    code = "Digit" + key;
    number = key.charCodeAt(0);
  } else if (key === " ") {
    code = "Space";
  } else if (/^F([1-9]|1[0-2])$/.test(key)) {
    code = key;
    number = 111 + Number(key.slice(1));
  } else if (key === "Shift" || key === "Control" || key === "Alt" || key === "Meta") {
    code = key + "Left";
  } else if (number) {
    code = key;
  }
  return { key: key, code: code, number: number, printable: Array.from(key).length === 1 };
}

function stroke(el, type, info, held) {
  const owner = view(el);
  const event = new owner.KeyboardEvent(type, {
    key: info.key,
    code: info.code,
    bubbles: true,
    cancelable: true,
    composed: true,
    view: owner,
    ctrlKey: !!held.ctrl,
    shiftKey: !!held.shift,
    altKey: !!held.alt,
    metaKey: !!held.meta
  });
  const letter = type === "keypress" && info.printable ? info.key.codePointAt(0) : 0;
  const number = letter || info.number;
  Object.defineProperty(event, "keyCode", { get: function () { return number; } });
  Object.defineProperty(event, "which", { get: function () { return number; } });
  Object.defineProperty(event, "charCode", { get: function () { return letter; } });
  return el.dispatchEvent(event);
}

function behave(el, info, held) {
  const key = info.key;
  if (held.ctrl || held.meta) {
    if (key.toLowerCase() === "a") mark(el);
    return;
  }
  if (key === "Enter") return enter(el);
  if (key === "Tab") return tab(el, !!held.shift);
  if (key === "Backspace" || key === "Delete") return erase(el, key === "Delete");
  if (key === "ArrowLeft" || key === "ArrowRight" || key === "Home" || key === "End") return move(el, key);
  if (key === " " && el.matches(PRESSABLE)) return el.click();
  if (info.printable) insert(el, key);
}

function keystroke(el, key, held, acting) {
  const info = keyed(key);
  const typed = (info.printable || key === "Enter") && !held.ctrl && !held.meta;
  let proceed = stroke(el, "keydown", info, held);
  if (proceed && typed) proceed = stroke(el, "keypress", info, held);
  if (proceed && acting) behave(el, info, held);
  stroke(active(el.ownerDocument) || el, "keyup", info, held);
}

function type(el, text) {
  ready(el, { visible: true, enabled: true });
  focus(el);
  const bulk = el.localName === "input" && el.type === "number";
  Array.from(text).forEach(function (letter) {
    keystroke(el, letter === "\n" ? "Enter" : letter, {}, !bulk);
  });
  if (bulk && enabled(el) && !locked(el)) {
    assign(el, el.value + text);
    announce(el, "insertText", text);
  }
  if (el.isConnected && editing(el) && editing(el) !== "rich") changed(el);
}

function press(el, arg) {
  ready(el, { visible: true, enabled: true });
  focus(el);
  keystroke(el, arg.key, arg.held, true);
}
