import React from "react";
import {
  AbsoluteFill,
  Img,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { BRAND } from "./theme";

/**
 * Seamless looping background (no text) for the landing hero <video>.
 * Uses sinusoidal motion so the first and last frame match exactly.
 */
export const HeroLoop: React.FC<{ src?: string }> = ({ src = "hero.png" }) => {
  const frame = useCurrentFrame();
  const { durationInFrames, width, height } = useVideoConfig();
  const tau = (2 * Math.PI * frame) / durationInFrames;

  const scale = 1.12 + 0.05 * Math.sin(tau);
  const driftX = 18 * Math.sin(tau);
  const driftY = 12 * Math.cos(tau);

  const particles = React.useMemo(
    () =>
      new Array(40).fill(0).map((_, i) => ({
        x: (i * 47) % 100,
        y: (i * 31) % 100,
        size: 2 + ((i * 5) % 5),
        loops: 1 + (i % 3),
        phase: (i % 8) * 0.7,
      })),
    []
  );

  return (
    <AbsoluteFill style={{ backgroundColor: BRAND.bg, overflow: "hidden" }}>
      <Img
        src={staticFile(src)}
        style={{
          width: "100%",
          height: "100%",
          objectFit: "cover",
          transform: `scale(${scale}) translate(${driftX}px, ${driftY}px)`,
          opacity: 0.7,
        }}
      />
      {particles.map((p, i) => {
        const t = tau * p.loops + p.phase;
        const y = (p.y + Math.sin(t) * 14 + 100) % 100;
        const tw = 0.35 + 0.5 * Math.abs(Math.sin(t));
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
              opacity: tw * 0.7,
              boxShadow: `0 0 ${p.size * 3}px ${BRAND.accent}`,
            }}
          />
        );
      })}
      <AbsoluteFill
        style={{
          background: `radial-gradient(${width * 0.55}px ${
            height * 0.55
          }px at ${50 + 16 * Math.sin(tau)}% ${42 + 12 * Math.cos(tau)}%, rgba(255,106,43,0.26), transparent 70%)`,
        }}
      />
      <AbsoluteFill
        style={{
          background:
            "radial-gradient(120% 120% at 50% 30%, transparent 45%, rgba(0,0,0,0.5) 100%)",
        }}
      />
    </AbsoluteFill>
  );
};
