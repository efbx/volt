function view(node) {
  return (node.ownerDocument || node).defaultView || window;
}

function ordered(nodes) {
  return Array.from(new Set(nodes)).sort(function (a, b) {
    if (a === b) return 0;
    return a.compareDocumentPosition(b) & 4 ? -1 : 1;
  });
}

function framed(el) {
  let inner = null;
  try {
    inner = el.contentDocument;
  } catch (error) {
    inner = null;
  }
  return inner || halt(fail("frame", "it is not a frame of the same site"));
}

function shadowed(el) {
  return el.shadowRoot || halt(fail("frame", "it has no open shadow root"));
}

function nearest(el, by) {
  if (by.strategy === "css") {
    try {
      return el.closest(by.value);
    } catch (error) {
      return halt(fail("selector", describe(error)));
    }
  }
  const matching = new Set(find(el.getRootNode(), by));
  for (let node = el; node; node = node.parentElement) {
    if (matching.has(node)) return node;
  }
  return null;
}

function named(scope, step) {
  if (step === "first") return scope.slice(0, 1);
  if (step === "last") return scope.slice(-1);
  if (step === "visible") return scope.filter(visible);
  if (step === "focused") return [active(document)].filter(Boolean);
  if (step === "frame") return scope.map(framed);
  if (step === "shadow") return scope.map(shadowed);
  if (step === "parent") {
    return ordered(scope.map(function (el) {
      return el.parentElement;
    }).filter(Boolean));
  }
  return halt(fail("script", "unknown step " + step));
}

function advance(scope, step) {
  if (typeof step === "string") return named(scope, step);
  if ("find" in step) {
    return ordered(scope.reduce(function (found, root) {
      return found.concat(find(root, step.find));
    }, []));
  }
  if ("nth" in step) return scope.slice(step.nth, step.nth + 1);
  if ("with_text" in step) {
    const wanted = fold(step.with_text);
    return scope.filter(function (el) {
      return fold(shown(el)).indexOf(wanted) >= 0;
    });
  }
  if ("without_text" in step) {
    const unwanted = fold(step.without_text);
    return scope.filter(function (el) {
      return fold(shown(el)).indexOf(unwanted) < 0;
    });
  }
  if ("having" in step) {
    return scope.filter(function (el) {
      return find(el, step.having).length > 0;
    });
  }
  if ("not_having" in step) {
    return scope.filter(function (el) {
      return find(el, step.not_having).length === 0;
    });
  }
  if ("or" in step) return ordered(scope.concat(resolve(step.or)));
  if ("closest" in step) {
    return ordered(scope.map(function (el) {
      return nearest(el, step.closest);
    }).filter(Boolean));
  }
  return halt(fail("script", "unknown step"));
}

function resolve(path) {
  return path.reduce(advance, [document]).filter(function (node) {
    return node.nodeType === 1;
  });
}

function one() {
  const found = resolve(command.path);
  if (found.length === 0) return halt(wait("missing"));
  if (found.length > 1) return halt(fail("ambiguous", "", found.length));
  return found[0];
}
