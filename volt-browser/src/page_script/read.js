function reached(state) {
  const found = resolve(command.path);
  if (state === "detached") return found.length === 0;
  if (state === "hidden" && found.length === 0) return true;
  if (found.length === 0) return halt(wait("missing"));
  if (found.length > 1) return halt(fail("ambiguous", "", found.length));
  const el = found[0];
  if (state === "attached") return true;
  if (state === "visible") return visible(el);
  if (state === "hidden") return !visible(el);
  if (state === "enabled") return enabled(el);
  if (state === "disabled") return !enabled(el);
  if (state === "editable") return editable(el);
  if (state === "checked") return ticked(el);
  if (state === "unchecked") return !ticked(el);
  if (state === "focused") return active(el.ownerDocument) === el;
  return halt(fail("script", "unknown state " + state));
}

const listed = {
  exists: function (found) {
    return ok(found.length > 0);
  },
  count: function (found) {
    return ok(found.length);
  },
  texts: function (found) {
    return ok(found.map(function (el) {
      return shown(el).trim();
    }));
  },
  is_visible: function (found) {
    if (found.length > 1) return fail("ambiguous", "", found.length);
    return ok(found.length === 1 && visible(found[0]));
  }
};

const reads = {
  text: function (el) {
    return shown(el).trim();
  },
  text_content: function (el) {
    return el.textContent;
  },
  inner_html: function (el) {
    return el.innerHTML;
  },
  outer_html: function (el) {
    return el.outerHTML;
  },
  attribute: function (el, name) {
    return el.getAttribute(name);
  },
  property: function (el, name) {
    return el[name];
  },
  value: function (el) {
    return "value" in el ? String(el.value) : halt(fail("wrong", "it has no value"));
  },
  style: function (el, name) {
    return view(el).getComputedStyle(el).getPropertyValue(name);
  },
  rect: box,
  tag: function (el) {
    return el.localName;
  },
  is_enabled: enabled,
  is_editable: editable,
  is_checked: ticked,
  is_selected: function (el) {
    return el.localName === "option" ? el.selected : ticked(el);
  },
  is_focused: function (el) {
    return active(el.ownerDocument) === el;
  }
};
