import React from 'react';
import {AbsoluteFill, Sequence, useCurrentFrame} from 'remotion';
import {HighlightedCode} from 'codehike/code';
import {Window} from '../Window';
import {StaticCode} from '../StaticCode';
import {CodeTransition} from '../CodeTransition';
import {gh} from '../theme';

const HOLD = 24;
const CODE_BOX_H = 6 * 51;

export const Crime: React.FC<{
  clean: HighlightedCode;
  dirty: HighlightedCode;
}> = ({clean, dirty}) => {
  const frame = useCurrentFrame();
  return (
    <AbsoluteFill
      style={{
        background: gh.bg,
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      <Window title="example.ts">
        <div style={{minHeight: CODE_BOX_H}}>
          {frame < HOLD ? (
            <StaticCode code={clean} />
          ) : (
            <Sequence from={HOLD} layout="none">
              <CodeTransition
                oldCode={clean}
                newCode={dirty}
                durationInFrames={34}
              />
            </Sequence>
          )}
        </div>
      </Window>
    </AbsoluteFill>
  );
};
