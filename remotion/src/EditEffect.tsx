import React from "react";
import {
  AbsoluteFill,
  OffthreadVideo,
  staticFile,
  interpolate,
  useCurrentFrame,
  useVideoConfig,
} from "remotion";

export type EditEffectProps = {
  effectId: string;
  clipSeconds: number;
  fps: number;
};

/**
 * Previews a single editorial effect over the extracted clip
 * (`public/preview_src.mp4`). Each `effectId` matches the catalog in
 * `src-tauri/src/edit_agent.rs::effect_catalog`.
 */
export const EditEffect: React.FC<EditEffectProps> = ({ effectId }) => {
  const frame = useCurrentFrame();
  const { durationInFrames, fps } = useVideoConfig();
  const t = durationInFrames > 1 ? frame / (durationInFrames - 1) : 0;
  const beat = Math.sin((frame / fps) * Math.PI * 2 * 2); // ~2 hits/sec

  // Base transforms shared by branches.
  let scale = 1;
  let translateX = 0;
  let rotate = 0;
  let blur = 0;
  let brightness = 1;
  let contrast = 1;
  let saturate = 1;
  let sepia = 0;
  let hue = 0;
  const overlays: React.ReactNode[] = [];

  switch (effectId) {
    case "crossfade":
      brightness = interpolate(t, [0, 0.15, 0.85, 1], [0, 1, 1, 0]);
      break;
    case "whip-pan": {
      const mid = Math.abs(t - 0.5);
      blur = interpolate(mid, [0, 0.5], [22, 0]);
      translateX = interpolate(t, [0, 0.5, 1], [-180, 0, 0]);
      break;
    }
    case "zoom-punch":
      scale = beat > 0.7 ? 1.12 : 1.0;
      break;
    case "beat-flash":
      brightness = 1 + Math.max(0, beat) * 0.9;
      break;
    case "speed-ramp":
      scale = interpolate(t, [0, 0.6, 1], [1.05, 1.05, 1.18]);
      break;
    case "rgb-glitch": {
      const jitter = ((frame * 13) % 7) - 3;
      hue = jitter * 20;
      contrast = 1.3;
      saturate = 1.6;
      overlays.push(
        <AbsoluteFill
          key="g"
          style={{
            mixBlendMode: "screen",
            background:
              ((frame % 5) | 0) === 0
                ? "rgba(255,0,80,0.18)"
                : "rgba(0,200,255,0.12)",
          }}
        />
      );
      break;
    }
    case "light-leak":
      overlays.push(
        <AbsoluteFill
          key="l"
          style={{
            mixBlendMode: "screen",
            opacity: 0.5 + 0.3 * Math.sin(t * Math.PI),
            background:
              "radial-gradient(60% 60% at 80% 20%, rgba(255,170,60,0.8), transparent 70%)",
          }}
        />
      );
      break;
    case "film-burn":
      sepia = 0.4;
      contrast = 1.1;
      overlays.push(
        <AbsoluteFill
          key="f"
          style={{
            mixBlendMode: "overlay",
            opacity: 0.35,
            background:
              "radial-gradient(80% 80% at 50% 50%, transparent 55%, rgba(60,20,0,0.9))",
          }}
        />
      );
      break;
    case "lower-third":
      overlays.push(
        <AbsoluteFill
          key="lt"
          style={{ justifyContent: "flex-end", padding: 64 }}
        >
          <div
            style={{
              opacity: interpolate(t, [0, 0.1, 0.9, 1], [0, 1, 1, 0]),
              transform: `translateX(${interpolate(t, [0, 0.15], [-40, 0])}px)`,
              background: "rgba(0,0,0,0.6)",
              color: "#fff",
              fontFamily: "sans-serif",
              fontSize: 48,
              fontWeight: 700,
              padding: "16px 28px",
              borderLeft: "6px solid #ff2d95",
              maxWidth: "70%",
            }}
          >
            Artista — Track
          </div>
        </AbsoluteFill>
      );
      break;
    case "grade-warm":
      saturate = 1.25;
      sepia = 0.2;
      hue = -10;
      break;
    case "grade-noir":
      saturate = 0.1;
      contrast = 1.4;
      brightness = 0.95;
      break;
    default:
      break;
  }

  const filter = `blur(${blur}px) brightness(${brightness}) contrast(${contrast}) saturate(${saturate}) sepia(${sepia}) hue-rotate(${hue}deg)`;

  return (
    <AbsoluteFill style={{ background: "#000" }}>
      <AbsoluteFill
        style={{
          transform: `scale(${scale}) translateX(${translateX}px) rotate(${rotate}deg)`,
          filter,
        }}
      >
        <OffthreadVideo src={staticFile("preview_src.mp4")} />
      </AbsoluteFill>
      {overlays}
    </AbsoluteFill>
  );
};
