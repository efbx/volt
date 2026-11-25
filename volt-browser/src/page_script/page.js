const MARK = "__volt_browser_mark";

function leave(step) {
  return function () {
    const there = window.navigation;
    const none = there && there[step > 0 ? "canGoForward" : "canGoBack"] === false;
    if (none && history.length <= Math.max(there.entries().length, 1)) return ok(false);
    history.go(step);
    return ok(true);
  };
}

function encoded(bytes) {
  let text = "";
  for (let start = 0; start < bytes.length; start += 32768) {
    text += String.fromCharCode.apply(null, bytes.subarray(start, start + 32768));
  }
  return btoa(text);
}

async function cached(arg) {
  if (!(await caches.has(arg.cache))) return null;
  const answer = await (await caches.open(arg.cache)).match(arg.url);
  if (!answer) return null;
  return {
    status: answer.status,
    headers: Array.from(answer.headers.entries()),
    body: encoded(new Uint8Array(await answer.arrayBuffer()))
  };
}

const paged = {
  take: take,
  evaluate: function () {
    return settle(code());
  },
  until: function (arg) {
    return met(code(arg));
  },
  url: function () {
    return ok(location.href);
  },
  title: function () {
    return ok(document.title);
  },
  source: function () {
    return ok(document.documentElement ? document.documentElement.outerHTML : "");
  },
  user_agent: function () {
    return ok(String(navigator.userAgent));
  },
  mark: function () {
    Object.defineProperty(window, MARK, { value: command.token, configurable: true });
    return ok({ token: command.token, url: location.href });
  },
  moved: function (arg) {
    const marked = window[MARK] === arg.token;
    if ((marked && location.href === arg.url) || document.readyState !== "complete") return met(false);
    if (marked) delete window[MARK];
    return met(true);
  },
  loaded: function (arg) {
    return met(arg === "interactive" ? document.readyState !== "loading" : document.readyState === "complete");
  },
  at: function (arg) {
    const url = location.href;
    if ("exact" in arg) return met(url === arg.exact);
    if ("prefix" in arg) return met(url.indexOf(arg.prefix) === 0);
    return met(url.indexOf(arg.containing) >= 0);
  },
  reload: function () {
    location.reload();
    return ok(true);
  },
  back: leave(-1),
  forward: leave(1),
  stop: function () {
    window.stop();
    return ok(null);
  },
  write: function (arg) {
    document.open();
    document.write(arg);
    document.close();
    return ok(null);
  },
  storage_get: function (arg) {
    return ok(window[arg.area].getItem(arg.key));
  },
  storage_set: function (arg) {
    window[arg.area].setItem(arg.key, arg.value);
    return ok(null);
  },
  storage_remove: function (arg) {
    window[arg.area].removeItem(arg.key);
    return ok(null);
  },
  storage_clear: function (arg) {
    window[arg.area].clear();
    return ok(null);
  },
  storage_entries: function (arg) {
    const store = window[arg.area];
    const pairs = [];
    for (let index = 0; index < store.length; index += 1) {
      pairs.push([store.key(index), store.getItem(store.key(index))]);
    }
    return ok(pairs);
  },
  cache_names: function () {
    return settle(caches.keys());
  },
  cache_urls: function (arg) {
    return settle(caches.has(arg.cache).then(function (exists) {
      return exists ? caches.open(arg.cache).then(function (cache) {
        return cache.keys();
      }) : [];
    }).then(function (requests) {
      return requests.map(function (request) {
        return request.url;
      });
    }));
  },
  cache_get: function (arg) {
    return settle(cached(arg));
  },
  cache_delete: function (arg) {
    return settle(caches.delete(arg.cache));
  },
  cache_remove: function (arg) {
    return settle(caches.has(arg.cache).then(function (exists) {
      return exists ? caches.open(arg.cache).then(function (cache) {
        return cache.delete(arg.url);
      }) : false;
    }));
  }
};
