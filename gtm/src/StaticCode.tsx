import {HighlightedCode, Pre} from 'codehike/code';
import React from 'react';
import {fontFamily, fontSize, tabSize} from './font';

export const StaticCode: React.FC<{code: HighlightedCode}> = ({code}) => {
  return (
    <Pre
      code={code}
      style={{
        position: 'relative',
        margin: 0,
        fontSize,
        lineHeight: 1.5,
        fontFamily,
        tabSize,
        background: 'transparent',
      }}
    />
  );
};
