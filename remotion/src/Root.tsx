import React from "react";
import { Composition } from "remotion";
import { HeroPromo } from "./HeroPromo";
import { HeroLoop } from "./HeroLoop";

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
    </>
  );
};
