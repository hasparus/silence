import React from 'react';
import {CalculateMetadataFunction, Composition} from 'remotion';
import {Meme, MemeProps, TOTAL} from './Meme';
import {CODE_CLEAN, CODE_DIRTY, hl} from './highlight';
import {fontsReady} from './fonts';

const calculateMetadata: CalculateMetadataFunction<MemeProps> = async () => {
  await fontsReady;
  const [clean, dirty] = await Promise.all([hl(CODE_CLEAN), hl(CODE_DIRTY)]);
  return {props: {clean, dirty}};
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
