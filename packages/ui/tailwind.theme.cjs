/** Tailwind theme extension mapped from Ethics Launch tokens. */
module.exports = {
  theme: {
    extend: {
      colors: {
        ethics: {
          night: "#0B0D0F",
          graphite: "#12161A",
          pine: "#16211D",
          slate: "#2A3138",
          fog: "#A7B0B7",
          silver: "#D9DEE3",
          jade: "#63D6B4",
          teal: "#38BFA7",
          violet: "#7F7BFF",
          rose: "#F23674",
        },
        primary: "#38BFA7",
      },
      fontFamily: {
        display: ['"Cormorant Garamond"', "Georgia", "serif"],
        sans: ['"DM Sans"', "system-ui", "sans-serif"],
      },
      fontSize: {
        xxs: ["0.5rem", { lineHeight: "0.625rem" }],
        label: ["0.65rem", { lineHeight: "1rem", letterSpacing: "0.18em" }],
      },
      borderRadius: {
        xl: "0.75rem",
        "2xl": "1rem",
      },
      maxWidth: {
        "7xl": "80rem",
      },
    },
  },
};
