import React from 'react';
import {interpolate, useCurrentFrame} from 'remotion';

const rand = (i: number) => {
  const x = Math.sin(i * 127.1 + 311.7) * 43758.5453;
  return x - Math.floor(x);
};

export const FireBurn: React.FC<{
  start: number;
  duration: number;
  top: number;
  left: number;
  width: number;
  height: number;
}> = ({start, duration, top, left, width, height}) => {
  const frame = useCurrentFrame();
  const p = interpolate(frame, [start, start + duration], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  if (p <= 0 || p >= 1) return null;

  const baseline = top + height;
  const env = Math.sin(Math.PI * Math.min(1, p * 1.08));

  const NF = 22;
  const flames = Array.from({length: NF}).map((_, i) => {
    const fx = left + (i / (NF - 1)) * width;
    const ph = rand(i) * 6.28;
    const flick =
      0.6 + 0.4 * Math.sin(frame * 0.7 + ph) + 0.18 * Math.sin(frame * 1.4 + ph * 2);
    const h = (70 + rand(i + 9) * 95) * Math.max(0.2, flick) * env;
    const w = 30 + rand(i + 3) * 36;
    const rise = p * 46;
    return (
      <div
        key={i}
        style={{
          position: 'absolute',
          left: fx - w / 2,
          top: baseline - h - rise,
          width: w,
          height: h,
          background:
            'radial-gradient(ellipse 52% 62% at 50% 100%, #fff7cc 0%, #ffd21a 22%, #ff8a00 48%, #ff2200 74%, rgba(255,34,0,0) 100%)',
          borderRadius: '50% 50% 46% 46% / 64% 64% 36% 36%',
          filter: 'blur(3px)',
          mixBlendMode: 'screen',
          opacity: 0.92 * env,
        }}
      />
    );
  });

  const NE = 26;
  const embers = Array.from({length: NE}).map((_, i) => {
    const ex = left + rand(i) * width;
    const prog = (p + rand(i + 5)) % 1;
    const ey = baseline - prog * (height + 190);
    const s = 2 + rand(i + 2) * 4;
    return (
      <div
        key={'e' + i}
        style={{
          position: 'absolute',
          left: ex,
          top: ey,
          width: s,
          height: s,
          borderRadius: 99,
          background: '#ffb43a',
          opacity: (1 - prog) * env,
          filter: 'blur(0.5px)',
        }}
      />
    );
  });

  const smokeOp = interpolate(p, [0.2, 0.5, 0.85, 1], [0, 0.22, 0.16, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  const NS = 5;
  const smoke = Array.from({length: NS}).map((_, i) => {
    const sx = left + (i / (NS - 1)) * width;
    const rise = p * 130;
    const d = 70 + rand(i) * 50;
    return (
      <div
        key={'s' + i}
        style={{
          position: 'absolute',
          left: sx - d / 2,
          top: baseline - 30 - rise - d / 2,
          width: d,
          height: d,
          borderRadius: 99,
          background: '#9aa0a6',
          filter: 'blur(20px)',
          opacity: smokeOp * (0.6 + rand(i + 1) * 0.4),
        }}
      />
    );
  });

  const heat = interpolate(p, [0, 0.5], [0, 1], {extrapolateRight: 'clamp'});

  return (
    <div style={{position: 'absolute', inset: 0, pointerEvents: 'none'}}>
      <div
        style={{
          position: 'absolute',
          top,
          left,
          width,
          height,
          background: 'linear-gradient(90deg,#ff6a00,#ff2200)',
          mixBlendMode: 'overlay',
          opacity: 0.85 * heat * env,
        }}
      />
      {smoke}
      {flames}
      {embers}
    </div>
  );
};
