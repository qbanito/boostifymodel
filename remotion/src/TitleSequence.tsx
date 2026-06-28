import React from "react";
import {
  AbsoluteFill,
  interpolate,
  spring,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { BRAND, FONT, GRADIENT } from "./theme";

const Word: React.FC<{
  text: string;
  delay: number;
  gradient?: boolean;
  size: number;
  weight?: number;
  letterSpacing?: number;
}> = ({ text, delay, gradient, size, weight = 800, letterSpacing = -1 }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({ frame: frame - delay, fps, config: { damping: 200 } });
  const y = interpolate(s, [0, 1], [40, 0]);
  const opacity = interpolate(frame - delay, [0, 12], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  return (
    <div
      style={{
        fontSize: size,
        fontWeight: weight,
        letterSpacing,
        lineHeight: 1.05,
        transform: `translateY(${y}px)`,
        opacity,
        ...(gradient
          ? {
              background: GRADIENT,
              WebkitBackgroundClip: "text",
              backgroundClip: "text",
              color: "transparent",
            }
          : { color: BRAND.text }),
      }}
    >
      {text}
    </div>
  );
};

const Pill: React.FC<{ delay: number; label: string }> = ({ delay, label }) => {
  const frame = useCurrentFrame();
  const opacity = interpolate(frame - delay, [0, 14], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  return (
    <div
      style={{
        opacity,
        display: "inline-flex",
        alignItems: "center",
        gap: 12,
        padding: "12px 22px",
        borderRadius: 999,
        border: "1px solid rgba(255,255,255,0.16)",
        background: "rgba(255,255,255,0.05)",
        color: BRAND.text,
        fontSize: 24,
        fontWeight: 600,
        marginBottom: 36,
      }}
    >
      <span
        style={{
          width: 12,
          height: 12,
          borderRadius: "50%",
          background: BRAND.accent,
          boxShadow: `0 0 16px ${BRAND.accent}`,
        }}
      />
      {label}
    </div>
  );
};

/** Headline + tagline sequence used in the promo. */
export const TitleSequence: React.FC = () => {
  return (
    <AbsoluteFill
      style={{
        fontFamily: FONT,
        justifyContent: "center",
        alignItems: "center",
        textAlign: "center",
        padding: 80,
      }}
    >
      <Pill delay={6} label="Closed beta · Launching Q2 2026" />
      <Word text="BOOSTIFY MOTIONDNA" delay={16} size={84} letterSpacing={2} weight={900} />
      <div style={{ height: 18 }} />
      <Word text="The Motion Model Trained on" delay={32} size={56} weight={700} />
      <Word text="700+ Real Music Videos" delay={44} size={72} gradient weight={900} />
    </AbsoluteFill>
  );
};
