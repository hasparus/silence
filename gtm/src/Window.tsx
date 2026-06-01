import React from 'react';
import {gh, FONT} from './theme';

const Dot: React.FC<{c: string}> = ({c}) => (
  <span
    style={{width: 15, height: 15, borderRadius: 99, background: c, display: 'block'}}
  />
);

export const Window: React.FC<{
  title?: string;
  width?: number;
  children: React.ReactNode;
}> = ({title = 'example.ts', width = 940, children}) => {
  return (
    <div
      style={{
        width,
        background: gh.bg,
        borderRadius: 18,
        border: `1px solid ${gh.windowBorder}`,
        overflow: 'hidden',
        boxShadow: '0 30px 80px rgba(31,35,40,0.20)',
      }}
    >
      <div
        style={{
          height: 54,
          background: gh.windowBar,
          borderBottom: `1px solid ${gh.windowBorder}`,
          display: 'flex',
          alignItems: 'center',
          paddingLeft: 22,
          gap: 9,
        }}
      >
        <Dot c={gh.dotRed} />
        <Dot c={gh.dotYellow} />
        <Dot c={gh.dotGreen} />
        <span
          style={{
            marginLeft: 18,
            fontFamily: FONT,
            fontSize: 22,
            color: gh.gutterText,
          }}
        >
          {title}
        </span>
      </div>
      <div style={{padding: '36px 44px'}}>{children}</div>
    </div>
  );
};
