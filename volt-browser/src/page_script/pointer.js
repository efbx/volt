const QUIET = { mouseenter: true, mouseleave: true, pointerenter: true, pointerleave: true };
const BUTTON = { left: 0, middle: 1, right: 2 };
const MASK = { left: 1, middle: 4, right: 2 };

function mouse(el, type, point, extra) {
  const owner = view(el);
  const held = extra.held || {};
  const init = {
    bubbles: !QUIET[type],
    cancelable: !QUIET[type],
    composed: true,
    view: owner,
    detail: extra.detail || 0,
    clientX: point.x,
    clientY: point.y,
    screenX: point.x,
    screenY: point.y,
    button: extra.button || 0,
    buttons: extra.buttons || 0,
    ctrlKey: !!held.ctrl,
    shiftKey: !!held.shift,
    altKey: !!held.alt,
    metaKey: !!held.meta
  };
  if (type.indexOf("pointer") === 0 && owner.PointerEvent) {
    init.pointerId = 1;
    init.pointerType = "mouse";
    init.isPrimary = true;
    return el.dispatchEvent(new owner.PointerEvent(type, init));
  }
  return el.dispatchEvent(new owner.MouseEvent(type, init));
}

function over(el, point, held) {
  ["pointerover", "pointerenter", "mouseover", "mouseenter", "pointermove", "mousemove"].forEach(function (type) {
    mouse(el, type, point, { held: held });
  });
}

function strike(el, point, arg) {
  const button = BUTTON[arg.button];
  const held = arg.held;
  over(el, point, held);
  for (let count = 1; count <= arg.count; count += 1) {
    mouse(el, "pointerdown", point, { button: button, buttons: MASK[arg.button], held: held });
    if (mouse(el, "mousedown", point, { button: button, buttons: MASK[arg.button], detail: count, held: held })) engage(el);
    mouse(el, "pointerup", point, { button: button, held: held });
    mouse(el, "mouseup", point, { button: button, detail: count, held: held });
    if (button === 0) mouse(el, "click", point, { detail: count, held: held });
    else mouse(el, "auxclick", point, { button: button, detail: count, held: held });
    if (button === 2) mouse(el, "contextmenu", point, { button: button, held: held });
  }
  if (button === 0 && arg.count === 2) mouse(el, "dblclick", point, { detail: 2, held: held });
}

function click(el, arg) {
  strike(el, ready(el, { visible: true, still: true, enabled: true, reachable: true }, arg.offset), arg);
}

function hover(el) {
  over(el, ready(el, { visible: true, still: true, reachable: true }), {});
}
