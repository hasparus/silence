import React from 'react';
import {AbsoluteFill, useCurrentFrame} from 'remotion';
import {Window} from '../Window';
import {gh, FONT} from '../theme';

type Seg = {t: string; c: string};
const LINES: Seg[][] = [
  [
    {t: '$ ', c: gh.gutterText},
    {t: 'cargo install silence-cli', c: gh.text},
  ],
  [
    {t: '$ ', c: gh.gutterText},
    {t: 'silence hook install', c: gh.text},
  ],
  [{t: '  ✓ hook installed', c: '#1a7f37'}],
];

const START = 6;
const CHAR_FRAMES = 0.8;
const CODE_BOX_H = 3 * 51;

export const Install: React.FC = () => {
  const frame = useCurrentFrame();
  let budget = Math.max(0, Math.floor((frame - START) / CHAR_FRAMES));

  return (
    <AbsoluteFill
      style={{
        background: gh.bg,
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      <Window title="zsh — silence" width={940}>
        <div
          style={{
            minHeight: CODE_BOX_H,
            fontFamily: FONT,
            fontSize: 34,
            lineHeight: 1.5,
          }}
        >
          {LINES.map((segs, li) => {
            const rendered: React.ReactNode[] = [];
            let lineHasContent = false;
            for (let si = 0; si < segs.length; si++) {
              const {t, c} = segs[si];
              const shown = t.slice(0, Math.max(0, budget));
              budget -= t.length;
              if (shown.length > 0) lineHasContent = true;
              rendered.push(
                <span key={si} style={{color: c, whiteSpace: 'pre'}}>
                  {shown}
                </span>,
              );
            }
            return (
              <div key={li} style={{opacity: lineHasContent ? 1 : 0, height: 51}}>
                {rendered}
              </div>
            );
          })}
        </div>
      </Window>
    </AbsoluteFill>
  );
};
