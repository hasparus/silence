import React from 'react';
import {CalculateMetadataFunction, Composition} from 'remotion';
import {Meme, MemeProps, TOTAL} from './Meme';
import {CODE_BUGGY, CODE_DIRTY, CODE_CLEAN, hl} from './highlight';
import {fontsReady} from './fonts';

const calculateMetadata: CalculateMetadataFunction<MemeProps> = async () => {
  await fontsReady;
  const [buggy, dirty, clean] = await Promise.all([
    hl(CODE_BUGGY),
    hl(CODE_DIRTY),
    hl(CODE_CLEAN),
  ]);
  return {props: {buggy, dirty, clean}};
};

export const RemotionRoot: React.FC = () => {
  return (
    <Composition
      id="Meme"
      component={Meme}
      durationInFrames={TOTAL}
      fps={30}
      width={1080}
      height={1080}
      defaultProps={{} as MemeProps}
      calculateMetadata={calculateMetadata}
    />
  );
};
