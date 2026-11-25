const SLOTS = "__volt_browser_pending";

function Halt(envelope) {
  this.envelope = envelope;
}

function describe(error) {
  return error && error.message ? String(error.message) : String(error);
}

function fail(kind, message, count) {
  return { status: "error", kind: kind, message: message || "", count: count || 0 };
}

function wait(blocker, rect) {
  return { status: "wait", blocker: blocker, rect: rect || null };
}

function ok(value) {
  let json;
  try {
    json = JSON.stringify(value);
  } catch (error) {
    return fail("script", "the value cannot be turned into JSON: " + describe(error));
  }
  return { status: "ok", json: typeof json === "string" ? json : null };
}

function halt(envelope) {
  throw new Halt(envelope);
}

function met(holds) {
  return holds ? ok(null) : wait("unmet");
}

function settle(value) {
  if (!value || typeof value.then !== "function") return ok(value);
  let slots = window[SLOTS];
  if (!slots) {
    slots = {};
    Object.defineProperty(window, SLOTS, { value: slots, configurable: true });
  }
  const token = command.token;
  slots[token] = { status: "pending" };
  value.then(function (settled) {
    slots[token] = ok(settled);
  }, function (error) {
    slots[token] = fail("script", describe(error));
  });
  return { status: "pending" };
}

function take() {
  const slots = window[SLOTS];
  const slot = slots && slots[command.token];
  if (!slot) return fail("detached");
  if (slot.status === "pending") return slot;
  delete slots[command.token];
  if (Object.keys(slots).length === 0) delete window[SLOTS];
  return slot;
}

function run() {
  const op = command.op;
  const arg = command.arg;
  if (!command.path) {
    if (op === "call") return settle(code(arg));
    return paged[op] ? paged[op](arg) : fail("script", "unknown command " + op);
  }
  if (listed[op]) return listed[op](resolve(command.path));
  if (op === "state") return met(reached(arg));
  const el = one();
  if (op === "call") return settle(code(el, arg));
  if (op === "has_text") return met(fold(shown(el)).indexOf(fold(arg)) >= 0);
  if (reads[op]) return ok(reads[op](el, arg));
  if (!acts[op]) return fail("script", "unknown command " + op);
  acts[op](el, arg);
  return ok(null);
}
