let current = $state<{ message: string } | null>(null);
let resolvePending: ((discard: boolean) => void) | undefined;

export const discardPrompt = {
  get current() { return current; },
  request(message: string): Promise<boolean> {
    // Another close gesture must not broaden an existing tab confirmation into
    // approval to close every document in the window.
    if (current) return Promise.resolve(false);
    current = { message };
    return new Promise((resolve) => { resolvePending = resolve; });
  },
  answer(discard: boolean) {
    const resolve = resolvePending;
    resolvePending = undefined;
    current = null;
    resolve?.(discard);
  },
};
