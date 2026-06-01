const STATES = ['idle', 'composing', 'uploading', 'uploaded', 'generating', 'tar-ready', 'done'];

const NEXT_BY_STATE = {
  idle: 'composing',
  composing: 'uploading',
  uploading: 'uploaded',
  uploaded: 'generating',
  generating: 'tar-ready',
  'tar-ready': 'done',
  done: 'done',
};

const ALLOWED_OVERLAYS = new Set(['rate_limit', 'session_expired', 'stay_on_page', 'github_prompt_deny', 'github_prompt_read']);

export function createStateRegistry() {
  const registry = new Map();

  function ensure(id) {
    if (!registry.has(id)) {
      registry.set(id, {
        state: 'idle',
        overlays: new Set(),
        tarTargetName: 'jekko-fixes.tar.gz',
        history: [{ at: Date.now(), state: 'idle', overlays: [] }],
      });
    }
    return registry.get(id);
  }

  return {
    list() {
      return Array.from(registry.entries()).map(([id, value]) => ({
        id,
        state: value.state,
        overlays: Array.from(value.overlays),
        tarTargetName: value.tarTargetName,
      }));
    },
    read(id) {
      return ensure(id);
    },
    set(id, { state, overlays, tarTargetName } = {}) {
      const entry = ensure(id);
      if (state) {
        if (!STATES.includes(state)) {
          throw new Error(`invalid state: ${state}`);
        }
        entry.state = state;
      }
      if (Array.isArray(overlays)) {
        const filtered = overlays.filter((value) => ALLOWED_OVERLAYS.has(value));
        entry.overlays = new Set(filtered);
      }
      if (typeof tarTargetName === 'string' && tarTargetName.length > 0) {
        entry.tarTargetName = tarTargetName;
      }
      entry.history.push({
        at: Date.now(),
        state: entry.state,
        overlays: Array.from(entry.overlays),
      });
      return entry;
    },
    advance(id) {
      const entry = ensure(id);
      entry.state = NEXT_BY_STATE[entry.state] ?? entry.state;
      entry.history.push({
        at: Date.now(),
        state: entry.state,
        overlays: Array.from(entry.overlays),
      });
      return entry;
    },
    reset() {
      registry.clear();
    },
  };
}

export const STATES_ENUM = STATES;
export const OVERLAYS_ENUM = Array.from(ALLOWED_OVERLAYS);
