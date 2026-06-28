import React from "react";
import {
  AbsoluteFill,
  interpolate,
  Sequence,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";
import { MotionBackground } from "./MotionBackground";
import { TitleSequence } from "./TitleSequence";
import { BRAND, FONT, GRADIENT } from "./theme";

const Fade: React.FC<{
  children: React.ReactNode;
  inAt: number;
  outAt: number;
}> = ({ children, inAt, outAt }) => {
  const frame = useCurrentFrame();
  const opacity = interpolate(
    frame,
    [inAt, inAt + 14, outAt - 14, outAt],
    [0, 1, 1, 0],
    { extrapolateLeft: "clamp", extrapolateRight: "clamp" }
  );
  return <AbsoluteFill style={{ opacity }}>{children}</AbsoluteFill>;
};

const CheckRow: React.FC<{ text: string; delay: number }> = ({
  text,
  delay,
}) => {
  const frame = useCurrentFrame();
  const x = interpolate(frame - delay, [0, 16], [-30, 0], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  const opacity = interpolate(frame - delay, [0, 16], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 20,
        transform: `translateX(${x}px)`,
        opacity,
        fontSize: 40,
        color: BRAND.text,
        fontWeight: 600,
      }}
    >
      <span
        style={{
          width: 44,
          height: 44,
          borderRadius: 12,
          background: GRADIENT,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: "#160a02",
          fontWeight: 900,
          fontSize: 26,
        }}
      >
        ✓
      </span>
      {text}
    </div>
  );
};

const FeatureScene: React.FC = () => (
  <AbsoluteFill
    style={{
      fontFamily: FONT,
      justifyContent: "center",
      alignItems: "flex-start",
      padding: "0 140px",
      gap: 28,
    }}
  >
    <div
      style={{
        fontSize: 64,
        fontWeight: 900,
        color: BRAND.text,
        marginBottom: 20,
      }}
    >
      Movement, not just{" "}
      <span
        style={{
          background: GRADIENT,
          WebkitBackgroundClip: "text",
          backgroundClip: "text",
          color: "transparent",
        }}
      >
        images
      </span>
    </div>
    <CheckRow text="How an artist moves on stage" delay={12} />
    <CheckRow text="How choreography breathes" delay={24} />
    <CheckRow text="How the camera follows the performance" delay={36} />
  </AbsoluteFill>
);

const CtaScene: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const pulse = 1 + 0.04 * Math.sin((frame / fps) * 4);
  return (
    <AbsoluteFill
      style={{
        fontFamily: FONT,
        justifyContent: "center",
        alignItems: "center",
        textAlign: "center",
        gap: 40,
      }}
    >
      <div style={{ fontSize: 72, fontWeight: 900, color: BRAND.text }}>
        Download{" "}
        <span
          style={{
            background: GRADIENT,
            WebkitBackgroundClip: "text",
            backgroundClip: "text",
            color: "transparent",
          }}
        >
          Boostify
        </span>{" "}
        for desktop
      </div>
      <div
        style={{
          transform: `scale(${pulse})`,
          padding: "26px 56px",
          borderRadius: 18,
          background: GRADIENT,
          color: "#160a02",
          fontWeight: 900,
          fontSize: 40,
          boxShadow: "0 20px 60px rgba(255,106,43,0.4)",
        }}
      >
        Available for macOS & Windows
      </div>
      <div style={{ fontSize: 30, color: BRAND.muted, fontWeight: 600 }}>
        Closed beta · Launching Q2 2026
      </div>
    </AbsoluteFill>
  );
};

/** Full ~12s promo: title → features → CTA over the motion background. */
export const HeroPromo: React.FC = () => {
  return (
    <AbsoluteFill style={{ backgroundColor: BRAND.bg }}>
      <MotionBackground src="hero.png" />
      <Sequence from={0} durationInFrames={150}>
        <Fade inAt={0} outAt={150}>
          <TitleSequence />
        </Fade>
      </Sequence>
      <Sequence from={140} durationInFrames={130}>
        <Fade inAt={0} outAt={130}>
          <FeatureScene />
        </Fade>
      </Sequence>
      <Sequence from={260} durationInFrames={120}>
        <Fade inAt={0} outAt={120}>
          <CtaScene />
        </Fade>
      </Sequence>
    </AbsoluteFill>
  );
};
