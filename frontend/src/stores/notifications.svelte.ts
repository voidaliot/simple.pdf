let message = $state("");

export const notifications = {
  get message() { return message; },
  clear() { message = ""; },
  error(error: unknown) {
    message = error instanceof Error ? error.message : String(error);
  },
};
