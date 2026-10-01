// 画面の状態を持つ最小の外部ストア。
// パイプラインの実行は非同期で、途中結果 (④の一部完了など) を描きながら進むため、
// React の setState ではなく「いつでも最新値を読める」ストアにしてある。
// React 側は useSyncExternalStore で購読し、Vitest (node) はストアを直接読む。
export interface Store<T> {
  get: () => T;
  set: (update: Partial<T> | ((prev: T) => T)) => void;
  subscribe: (listener: () => void) => () => void;
}

export function createStore<T extends object>(initial: T): Store<T> {
  let state = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => state,
    set: (update) => {
      state = typeof update === 'function' ? update(state) : { ...state, ...update };
      for (const l of listeners) l();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
