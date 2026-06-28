import React from "react";
import { Composition } from "remotion";
import { HeroPromo } from "./HeroPromo";
import { HeroLoop } from "./HeroLoop";
import { EditEffect, EditEffectProps } from "./EditEffect";

export const RemotionRoot: React.FC = () => {
  return (
    <>
      <Composition
        id="HeroPromo"
        component={HeroPromo}
        durationInFrames={380}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="HeroLoop"
        component={HeroLoop}
        durationInFrames={180}
        fps={30}
        width={1920}
        height={1080}
      />
      <Composition
        id="EditEffect"
        component={EditEffect}
        durationInFrames={120}
        fps={30}
        width={1280}
        height={720}
        defaultProps={{ effectId: "crossfade", clipSeconds: 4, fps: 30 }}
        calculateMetadata={({ props }: { props: EditEffectProps }) => ({
          durationInFrames: Math.max(
            15,
            Math.round((props.clipSeconds || 4) * (props.fps || 30))
          ),
          fps: props.fps || 30,
        })}
      />
    </>
  );
};
