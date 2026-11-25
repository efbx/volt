function dispatch(el, arg) {
  const owner = view(el);
  const type = arg.type;
  const init = Object.assign({ bubbles: true, cancelable: true, composed: true }, arg.init || {});
  let Kind = owner.Event;
  if (/^(mouse|click$|dblclick$|contextmenu$|auxclick$)/.test(type)) Kind = owner.MouseEvent;
  else if (/^pointer/.test(type)) Kind = owner.PointerEvent || owner.MouseEvent;
  else if (/^key/.test(type)) Kind = owner.KeyboardEvent;
  else if (/^(focus|blur)/.test(type)) Kind = owner.FocusEvent;
  else if (/^(input|beforeinput)$/.test(type)) Kind = owner.InputEvent;
  else if ("detail" in init) Kind = owner.CustomEvent;
  if (Kind !== owner.Event && Kind !== owner.CustomEvent) init.view = owner;
  el.dispatchEvent(new Kind(type, init));
}

const acts = {
  click: click,
  hover: hover,
  focus: focus,
  blur: blur,
  fill: fill,
  type_text: type,
  press: press,
  set_checked: tick,
  select: choose,
  submit: submit,
  scroll_into_view: function (el) {
    el.scrollIntoView({ block: "center", inline: "center" });
  },
  scroll_to: function (el, point) {
    el.scrollTo(point.x, point.y);
  },
  scroll_by: function (el, point) {
    el.scrollBy(point.x, point.y);
  },
  select_text: function (el) {
    focus(el);
    mark(el);
  },
  dispatch: dispatch
};
