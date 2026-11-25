const SKIPPED = { SCRIPT: true, STYLE: true, NOSCRIPT: true, TEMPLATE: true };
const BUTTON_INPUTS = { button: true, submit: true, reset: true };

function squash(text) {
  return String(text === null || text === undefined ? "" : text).replace(/\s+/g, " ").trim();
}

function fold(text) {
  return squash(text).toLowerCase();
}

function quote(value) {
  return '"' + String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"') + '"';
}

function query(root, selector) {
  try {
    return Array.from(root.querySelectorAll(selector));
  } catch (error) {
    return halt(fail("selector", describe(error)));
  }
}

function content(el) {
  if (el.localName === "input" && BUTTON_INPUTS[el.type]) return el.value;
  return el.textContent;
}

function shown(el) {
  return typeof el.innerText === "string" ? el.innerText : el.textContent || "";
}

function deepest(root, matches) {
  return query(root, "*").filter(function (el) {
    if (SKIPPED[el.tagName] || !matches(content(el))) return false;
    return !Array.from(el.children).some(function (child) {
      return !SKIPPED[child.tagName] && matches(content(child));
    });
  });
}

function holding(attribute) {
  return function (root, value) {
    const wanted = fold(value);
    return query(root, "[" + attribute + "]").filter(function (el) {
      return fold(el.getAttribute(attribute)).indexOf(wanted) >= 0;
    });
  };
}

function labelled(root, value) {
  const wanted = fold(value);
  const found = [];
  query(root, "label").forEach(function (label) {
    if (label.control && fold(label.textContent).indexOf(wanted) >= 0) found.push(label.control);
  });
  query(root, "[aria-label]").forEach(function (el) {
    if (fold(el.getAttribute("aria-label")).indexOf(wanted) >= 0) found.push(el);
  });
  query(root, "[aria-labelledby]").forEach(function (el) {
    const owner = el.getRootNode();
    const text = el.getAttribute("aria-labelledby").split(/\s+/).map(function (id) {
      const named = id && owner.getElementById ? owner.getElementById(id) : null;
      return named ? named.textContent : "";
    }).join(" ");
    if (fold(text).indexOf(wanted) >= 0) found.push(el);
  });
  return found;
}

function xpath(root, value) {
  if (root.nodeType === 11) return [];
  const owner = root.ownerDocument || root;
  let found;
  try {
    found = owner.evaluate(value, root, null, 7, null);
  } catch (error) {
    return halt(fail("selector", describe(error)));
  }
  const nodes = [];
  for (let index = 0; index < found.snapshotLength; index += 1) {
    const node = found.snapshotItem(index);
    if (node.nodeType === 1) nodes.push(node);
  }
  return nodes;
}

const finders = {
  css: query,
  xpath: xpath,
  id: function (root, value) {
    return query(root, "#" + CSS.escape(value));
  },
  name: function (root, value) {
    return query(root, "[name=" + quote(value) + "]");
  },
  tag: function (root, value) {
    return query(root, CSS.escape(value));
  },
  class: function (root, value) {
    return query(root, "." + CSS.escape(value));
  },
  text: function (root, value) {
    const wanted = squash(value);
    return deepest(root, function (text) {
      return squash(text) === wanted;
    });
  },
  text_containing: function (root, value) {
    const wanted = fold(value);
    return deepest(root, function (text) {
      return fold(text).indexOf(wanted) >= 0;
    });
  },
  link: function (root, value) {
    const wanted = squash(value);
    return query(root, "a[href]").filter(function (link) {
      return squash(shown(link)) === wanted;
    });
  },
  link_containing: function (root, value) {
    const wanted = fold(value);
    return query(root, "a[href]").filter(function (link) {
      return fold(shown(link)).indexOf(wanted) >= 0;
    });
  },
  label: labelled,
  placeholder: holding("placeholder"),
  alt: holding("alt"),
  title: holding("title"),
  test_id: function (root, value) {
    return query(root, "[data-testid=" + quote(value) + "]");
  }
};

function find(root, by) {
  return finders[by.strategy](root, by.value);
}
