import React from 'react';
import {
  AbsoluteFill,
  Img,
  interpolate,
  spring,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from 'remotion';
import {FONT} from '../theme';

export const Silence: React.FC = () => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();

  const kenBurns = interpolate(frame, [0, 65], [1.08, 1.0], {
    extrapolateRight: 'clamp',
  });

  const flash = interpolate(frame, [0, 4], [0.85, 0], {
    extrapolateRight: 'clamp',
  });

  const wordIn = spring({
    frame: frame - 8,
    fps,
    config: {damping: 14, stiffness: 180},
  });
  const wordScale = interpolate(wordIn, [0, 1], [0.7, 1]);

  return (
    <AbsoluteFill style={{background: '#000'}}>
      <Img
        src={staticFile('king.png')}
        style={{
          width: '100%',
          height: '100%',
          objectFit: 'cover',
          objectPosition: 'center 28%',
          transform: `scale(${kenBurns})`,
        }}
      />
      <AbsoluteFill
        style={{
          background:
            'linear-gradient(to bottom, rgba(0,0,0,0) 52%, rgba(0,0,0,0.72) 100%)',
        }}
      />
      <AbsoluteFill
        style={{
          alignItems: 'center',
          justifyContent: 'flex-end',
          paddingBottom: 96,
        }}
      >
        <div
          style={{
            fontFamily: FONT,
            fontWeight: 700,
            fontSize: 116,
            color: '#fff',
            letterSpacing: -2,
            transform: `scale(${wordScale})`,
            opacity: wordIn,
            textShadow: '0 6px 40px rgba(0,0,0,0.65)',
            display: 'flex',
            alignItems: 'center',
            gap: 18,
          }}
        >
          <span style={{fontSize: 96}}>✋</span>
          <span>silence</span>
        </div>
      </AbsoluteFill>
      <AbsoluteFill style={{background: '#fff', opacity: flash}} />
    </AbsoluteFill>
  );
};
