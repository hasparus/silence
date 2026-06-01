import React from 'react';
import {AbsoluteFill, Sequence} from 'remotion';
import {HighlightedCode} from 'codehike/code';
import {Crime} from './scenes/Crime';
import {Silence} from './scenes/Silence';
import {Install} from './scenes/Install';
import {Cleanup} from './scenes/Cleanup';
import {EndCard} from './scenes/EndCard';

export type MemeProps = {
  clean: HighlightedCode;
  dirty: HighlightedCode;
};

export const SCENES = {
  crime: {from: 0, dur: 95},
  silence: {from: 95, dur: 65},
  install: {from: 160, dur: 95},
  cleanup: {from: 255, dur: 90},
  endcard: {from: 345, dur: 45},
};
export const TOTAL = SCENES.endcard.from + SCENES.endcard.dur;

export const Meme: React.FC<MemeProps> = ({clean, dirty}) => {
  return (
    <AbsoluteFill style={{background: '#ffffff'}}>
      <Sequence
        from={SCENES.crime.from}
        durationInFrames={SCENES.crime.dur}
        premountFor={30}
      >
        <Crime clean={clean} dirty={dirty} />
      </Sequence>
      <Sequence
        from={SCENES.silence.from}
        durationInFrames={SCENES.silence.dur}
        premountFor={30}
      >
        <Silence />
      </Sequence>
      <Sequence
        from={SCENES.install.from}
        durationInFrames={SCENES.install.dur}
        premountFor={30}
      >
        <Install />
      </Sequence>
      <Sequence
        from={SCENES.cleanup.from}
        durationInFrames={SCENES.cleanup.dur}
        premountFor={30}
      >
        <Cleanup clean={clean} dirty={dirty} />
      </Sequence>
      <Sequence
        from={SCENES.endcard.from}
        durationInFrames={SCENES.endcard.dur}
        premountFor={30}
      >
        <EndCard />
      </Sequence>
    </AbsoluteFill>
  );
};
