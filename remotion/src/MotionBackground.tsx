import React from "react";
import {
  AbsoluteFill,
  Img,
  interpolate,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { BRAND } from "./theme";

/**
 * Slow Ken Burns zoom over a generated motion-capture image plus a moving
 * brand-gradient glow and a drifting field of motion-trail particles.
 * Reused as the background of every composition.
 */
export const MotionBackground: React.FC<{ src?: string }> = ({
  src = "hero.png",
}) => {
  const frame = useCurrentFrame();
  const { durationInFrames, width, height } = useVideoConfig();

  const progress = frame / Math.max(1, durationInFrames - 1);
  const scale = interpolate(progress, [0, 1], [1.08, 1.22]);
  const drift = interpolate(progress, [0, 1], [-20, 20]);

  const particles = React.useMemo(() => {
    return new Array(36).fill(0).map((_, i) => ({
      x: (i * 53) % 100,
      y: (i * 29) % 100,
      size: 2 + ((i * 7) % 5),
      speed: 0.4 + ((i % 5) * 0.18),
      phase: (i % 7) * 0.9,
    }));
  }, []);

  return (
    <AbsoluteFill style={{ backgroundColor: BRAND.bg, overflow: "hidden" }}>
      <Img
        src={staticFile(src)}
        style={{
          width: "100%",
          height: "100%",
          objectFit: "cover",
          transform: `scale(${scale}) translateX(${drift}px)`,
          opacity: 0.62,
        }}
      />

      {/* particles drifting upward like motion-capture dots */}
      {particles.map((p, i) => {
        const t = (frame * p.speed) / 30 + p.phase;
        const y = (p.y - (t * 8) % 120 + 120) % 120;
        const twinkle = 0.3 + 0.5 * Math.abs(Math.sin(t * 1.7));
        return (
          <div
            key={i}
            style={{
              position: "absolute",
              left: `${p.x}%`,
              top: `${y}%`,
              width: p.size,
              height: p.size,
              borderRadius: "50%",
              background: i % 3 === 0 ? BRAND.accent2 : BRAND.accent,
              opacity: twinkle * 0.7,
              filter: "blur(0.4px)",
              boxShadow: `0 0 ${p.size * 3}px ${BRAND.accent}`,
            }}
          />
        );
      })}

      {/* moving radial glow */}
      <AbsoluteFill
        style={{
          background: `radial-gradient(${width * 0.5}px ${height * 0.5}px at ${
            50 + Math.sin(frame / 40) * 18
          }% ${40 + Math.cos(frame / 50) * 14}%, rgba(255,106,43,0.28), transparent 70%)`,
        }}
      />

      {/* vignette + bottom veil for text contrast */}
      <AbsoluteFill
        style={{
          background:
            "radial-gradient(120% 120% at 50% 30%, transparent 40%, rgba(0,0,0,0.55) 100%), linear-gradient(180deg, rgba(0,0,0,0.2), rgba(0,0,0,0.75))",
        }}
      />
    </AbsoluteFill>
  );
};
