// The native port in Safari.
//
// Chrome, Edge and Firefox start udm-host themselves (runtime.connectNative)
// and keep a port to it open both ways. Safari hands native messages to the
// app extension inside UDM.app (installer/macos/safari), which can answer a
// message but cannot send one of its own. So in Safari the worker talks
// through pollingPort, which looks like the port connectNative returns: each
// message goes out with runtime.sendNativeMessage, and when there is nothing
// to send it asks again now and then with an empty request. The app
// extension keeps one udm-host running and answers each request with every
// frame udm-host wrote since the last one.
//
// A request is { frames: [message, ...] }; its answer is { session, frames,
// more, error }: `session` names the udm-host that answered, `more` says it
// has further frames already, and `error` that it is gone and why.

// How often the port asks while UDM has something to say, and while nothing
// has happened for SAFARI_BUSY_FOR_MS.
var SAFARI_POLL_BUSY_MS = 1000;
var SAFARI_POLL_IDLE_MS = 5000;
var SAFARI_BUSY_FOR_MS = 15000;

// A port over `sendNative(request)`, which sends one request to the app
// extension and resolves to its answer. `clock` replaces setTimeout,
// clearTimeout and Date.now in tests. Like a port of connectNative's, it has
// postMessage, disconnect, onMessage and onDisconnect; disconnect stops it
// without telling onDisconnect, and onDisconnect's listeners get the port,
// whose `udmReason` says why it closed.
function pollingPort(sendNative, clock = { setTimeout, clearTimeout, now: () => Date.now() }) {
  const messageListeners = [];
  const disconnectListeners = [];
  const queue = [];
  let timer = null;
  let inFlight = false;
  let again = false;
  let closed = false;
  let session = null;
  let lastActivity = clock.now();

  const listeners = (list) => ({
    addListener: (f) => void list.push(f),
    removeListener: (f) => {
      const i = list.indexOf(f);
      if (i >= 0) list.splice(i, 1);
    },
    hasListener: (f) => list.includes(f),
  });

  const port = {
    udmReason: "",
    onMessage: listeners(messageListeners),
    onDisconnect: listeners(disconnectListeners),
    postMessage(message) {
      if (closed) throw new Error("Attempting to use a disconnected port object");
      queue.push(message);
      lastActivity = clock.now();
      schedule(0);
    },
    disconnect() {
      stop();
    },
  };

  function stop() {
    closed = true;
    if (timer !== null) clock.clearTimeout(timer);
    timer = null;
  }

  function close(reason) {
    if (closed) return;
    stop();
    port.udmReason = reason;
    for (const f of [...disconnectListeners]) {
      try {
        f(port);
      } catch (_) {
        // One listener's fault is not the port's.
      }
    }
  }

  function schedule(ms) {
    if (closed) return;
    if (timer !== null) clock.clearTimeout(timer);
    timer = clock.setTimeout(exchange, ms);
  }

  function exchange() {
    timer = null;
    if (closed) return;
    if (inFlight) {
      again = true;
      return;
    }
    inFlight = true;
    again = false;
    const frames = queue.splice(0);
    let answer;
    try {
      answer = Promise.resolve(sendNative({ frames }));
    } catch (e) {
      answer = Promise.reject(e);
    }
    answer.then(received, (e) => {
      inFlight = false;
      close((e && e.message) || String(e || "") || "no answer from UDM");
    });
  }

  function received(reply) {
    inFlight = false;
    if (closed) return;
    if (!reply || typeof reply !== "object") {
      close("no answer from UDM");
      return;
    }
    if (typeof reply.session === "string" && reply.session) {
      // Another udm-host never heard this port's hello: start over.
      if (session !== null && reply.session !== session) {
        close("UDM's host started again");
        return;
      }
      session = reply.session;
    }
    const frames = Array.isArray(reply.frames) ? reply.frames.filter((f) => f && typeof f === "object") : [];
    if (frames.length) lastActivity = clock.now();
    for (const frame of frames) {
      for (const f of [...messageListeners]) {
        try {
          f(frame, port);
        } catch (_) {
          // As a browser's port does, the next listener and frame still run.
        }
      }
      if (closed) return;
    }
    if (reply.error) {
      close(String(reply.error));
      return;
    }
    if (queue.length || again || reply.more === true) schedule(0);
    else schedule(clock.now() - lastActivity < SAFARI_BUSY_FOR_MS ? SAFARI_POLL_BUSY_MS : SAFARI_POLL_IDLE_MS);
  }

  return port;
}

if (typeof module !== "undefined") {
  module.exports = { pollingPort, SAFARI_POLL_BUSY_MS, SAFARI_POLL_IDLE_MS, SAFARI_BUSY_FOR_MS };
}
