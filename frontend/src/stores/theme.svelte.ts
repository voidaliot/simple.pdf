const KEY = "simplepdf:theme";

export type ThemeChoice = "system" | "light" | "dark";

function createThemeStore() {
  let choice = $state<ThemeChoice>(
    (localStorage.getItem(KEY) as ThemeChoice | null) ?? "system"
  );

  function apply(c: ThemeChoice) {
    const root = document.documentElement;
    if (c === "system") {
      root.removeAttribute("data-theme");
    } else {
      root.setAttribute("data-theme", c);
    }
  }

  // Apply on load
  apply(choice);

  function set(c: ThemeChoice) {
    choice = c;
    localStorage.setItem(KEY, c);
    apply(c);
  }

  return {
    get choice() { return choice; },
    set,
  };
}

export const theme = createThemeStore();
