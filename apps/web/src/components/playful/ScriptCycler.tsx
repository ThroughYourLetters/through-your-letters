import React, { useEffect, useState } from "react";
import { usePrefersReducedMotion } from "./usePrefersReducedMotion";

// "Hand" in the scripts the archive collects, shown one after another
const WORDS = [
  { text: "a hand", lang: "English", font: "inherit" },
  { text: "ಕೈ", lang: "Kannada", font: '"Noto Sans Kannada"' },
  { text: "हाथ", lang: "Hindi", font: '"Noto Sans Devanagari"' },
  { text: "கை", lang: "Tamil", font: '"Noto Sans Tamil"' },
  { text: "হাত", lang: "Bengali", font: '"Noto Sans Bengali"' },
  { text: "చేయి", lang: "Telugu", font: '"Noto Sans Telugu"' },
  { text: "ہاتھ", lang: "Urdu", font: '"Noto Nastaliq Urdu"' },
  { text: "കൈ", lang: "Malayalam", font: '"Noto Sans Malayalam"' },
  { text: "ହାତ", lang: "Odia", font: '"Noto Sans Odia"' },
];

const INTERVAL_MS = 2200;

const ScriptCycler: React.FC<{ className?: string }> = ({ className = "" }) => {
  const reduced = usePrefersReducedMotion();
  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);

  useEffect(() => {
    if (reduced || paused) return;
    const timer = window.setInterval(
      () => setIndex((i) => (i + 1) % WORDS.length),
      INTERVAL_MS,
    );
    return () => window.clearInterval(timer);
  }, [reduced, paused]);

  const word = WORDS[index];

  return (
    <button
      type="button"
      onClick={() => setIndex((i) => (i + 1) % WORDS.length)}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
      aria-label={`"hand" in ${word.lang}. Click for another script.`}
      title="Click to see another script"
      className={`group relative inline-grid items-baseline cursor-pointer align-baseline text-left leading-[inherit] ${className}`}
    >
      {/* Every word sits in the same grid cell so the headline never reflows */}
      {WORDS.map((w, i) => (
        <span
          key={w.lang}
          aria-hidden={i !== index}
          style={{
            fontFamily: w.font,
            fontStyle: w.font === "inherit" ? undefined : "normal",
            gridArea: "1 / 1",
          }}
          className={`whitespace-nowrap leading-[1.02] transition-all duration-500 ease-out ${
            i === index
              ? "opacity-100 translate-y-0 blur-0"
              : "pointer-events-none opacity-0 translate-y-3 blur-sm"
          }`}
        >
          {w.text}
        </span>
      ))}
      <span className="pointer-events-none absolute -bottom-1 left-0 translate-y-full rounded-full bg-ink px-2 py-0.5 font-sans text-[11px] font-medium not-italic tracking-normal text-paper opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
        {word.lang} · click to change
      </span>
    </button>
  );
};

export default ScriptCycler;
