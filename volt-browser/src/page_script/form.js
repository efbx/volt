function submit(el) {
  const form = el.localName === "form" ? el : el.form || el.closest("form");
  if (!form) halt(fail("wrong", "it is not inside a form"));
  if (form.requestSubmit) form.requestSubmit();
  else form.submit();
}

function ticked(el) {
  if (el.localName === "input" && (el.type === "checkbox" || el.type === "radio")) return el.checked;
  const state = el.getAttribute("aria-checked");
  if (state === null) return halt(fail("wrong", "it is not a checkbox or a radio button"));
  return state === "true";
}

function tick(el, wanted) {
  if (ticked(el) === wanted) return;
  if (!wanted && el.localName === "input" && el.type === "radio") {
    halt(fail("wrong", "a radio button cannot be unchecked"));
  }
  const point = ready(el, { visible: true, still: true, enabled: true, reachable: true });
  strike(el, point, { button: "left", count: 1, held: {} });
  if (ticked(el) !== wanted) halt(fail("wrong", "the click did not change its state"));
}

function choose(el, choices) {
  if (el.localName !== "select") halt(fail("wrong", "it is not a select element"));
  ready(el, { visible: true, enabled: true });
  if (!el.multiple && choices.length !== 1) halt(fail("wrong", "it takes exactly one option"));
  const options = Array.from(el.options);
  const chosen = choices.map(function (choice) {
    const option = "index" in choice ? options[choice.index] : options.filter(function (candidate) {
      if ("value" in choice) return candidate.value === choice.value;
      return squash(candidate.label || candidate.text) === squash(choice.label);
    })[0];
    return option || halt(fail("wrong", "it has no such option"));
  });
  options.forEach(function (option) {
    option.selected = chosen.indexOf(option) >= 0;
  });
  el.dispatchEvent(new (view(el).Event)("input", { bubbles: true }));
  changed(el);
}
