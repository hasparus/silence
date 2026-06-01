import React from 'react';
import {
  AbsoluteFill,
  interpolate,
  spring,
  useCurrentFrame,
  useVideoConfig,
} from 'remotion';
import {gh, FONT} from '../theme';

export const EndCard: React.FC = () => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();

  const inSpring = spring({frame, fps, config: {damping: 16, stiffness: 160}});
  const y = interpolate(inSpring, [0, 1], [40, 0]);
  const cmd = spring({frame: frame - 10, fps, config: {damping: 18}});

  return (
    <AbsoluteFill
      style={{
        background: gh.bg,
        alignItems: 'center',
        justifyContent: 'center',
        flexDirection: 'column',
        gap: 40,
      }}
    >
      <div
        style={{
          fontFamily: FONT,
          fontWeight: 700,
          fontSize: 132,
          color: gh.brand,
          letterSpacing: -3,
          opacity: inSpring,
          transform: `translateY(${y}px)`,
          display: 'flex',
          alignItems: 'center',
          gap: 22,
        }}
      >
        <span style={{fontSize: 108}}>✋</span>
        <span>silence</span>
      </div>
      <div
        style={{
          fontFamily: FONT,
          fontSize: 38,
          color: gh.text,
          background: gh.windowBar,
          border: `1px solid ${gh.windowBorder}`,
          borderRadius: 12,
          padding: '18px 30px',
          opacity: cmd,
          transform: `scale(${interpolate(cmd, [0, 1], [0.94, 1])})`,
        }}
      >
        <span style={{color: gh.gutterText}}>$ </span>
        cargo install silence-cli
      </div>
      <div
        style={{
          fontFamily: FONT,
          fontSize: 28,
          color: gh.gutterText,
          opacity: interpolate(frame, [18, 32], [0, 1], {
            extrapolateLeft: 'clamp',
            extrapolateRight: 'clamp',
          }),
        }}
      >
        let the code speak.
      </div>
    </AbsoluteFill>
  );
};
