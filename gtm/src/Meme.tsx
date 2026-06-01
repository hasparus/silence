import React from 'react';
import {
  AbsoluteFill,
  Img,
  Sequence,
  interpolate,
  spring,
  staticFile,
  useCurrentFrame,
  useVideoConfig,
} from 'remotion';
import {HighlightedCode} from 'codehike/code';
import {StaticCode} from './StaticCode';
import {CodeTransition} from './CodeTransition';
import {gh, FONT} from './theme';

export type MemeProps = {
  buggy: HighlightedCode;
  dirty: HighlightedCode;
  clean: HighlightedCode;
};

const T = {
  userStart: 8,
  agentStart: 46,
  edit1: 84,
  discGone: 108,
  king: 120,
  cargo: 156,
  hook: 184,
  edit2: 212,
};
const MORPH = 22;
const CPS = 1.5;
export const TOTAL = T.edit2 + MORPH + 42;

const CODE_LINES = 5;

const typed = (frame: number, start: number, len: number) =>
  Math.max(0, Math.min(len, Math.floor((frame - start) * CPS)));

const Caret: React.FC = () => {
  const frame = useCurrentFrame();
  const on = Math.floor(frame / 8) % 2 === 0;
  return (
    <span
      style={{
        display: 'inline-block',
        width: '0.6em',
        height: '1.05em',
        transform: 'translateY(0.18em)',
        background: gh.text,
        opacity: on ? 0.85 : 0,
        marginLeft: 1,
      }}
    />
  );
};

const Bubble: React.FC<{
  side: 'user' | 'agent';
  start: number;
  children: React.ReactNode;
  pad?: string;
}> = ({side, start, children, pad = '14px 22px'}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const s = spring({
    frame: frame - start,
    fps,
    config: {damping: 18, stiffness: 220},
  });
  const user = side === 'user';
  return (
    <div
      style={{
        alignSelf: user ? 'flex-end' : 'flex-start',
        maxWidth: 660,
        background: user ? '#0969da' : '#f1f2f4',
        color: user ? '#fff' : gh.text,
        fontFamily: FONT,
        fontSize: 30,
        lineHeight: 1.45,
        padding: pad,
        borderRadius: user ? '22px 22px 8px 22px' : '22px 22px 22px 8px',
        opacity: s,
        transform: `translateY(${interpolate(s, [0, 1], [18, 0])}px)`,
        whiteSpace: 'pre-wrap',
      }}
    >
      {children}
    </div>
  );
};

const Rainbow: React.FC<{text: string; opacity: number}> = ({text, opacity}) => {
  const frame = useCurrentFrame();
  return (
    <span style={{opacity}}>
      {text.split('').map((ch, i) => {
        const hue = (((i * 26 - frame * 9) % 360) + 360) % 360;
        return (
          <span
            key={i}
            style={{display: 'inline-block', whiteSpace: 'pre', color: `hsl(${hue}, 90%, 52%)`}}
          >
            {ch}
          </span>
        );
      })}
    </span>
  );
};

const CodeArea: React.FC<MemeProps> = ({buggy, dirty, clean}) => {
  const frame = useCurrentFrame();
  const minHeight = CODE_LINES * Math.round(30 * 1.5);
  let inner: React.ReactNode;
  if (frame < T.edit1) {
    inner = <StaticCode code={buggy} />;
  } else if (frame < T.edit2) {
    inner = (
      <Sequence from={T.edit1} layout="none">
        <CodeTransition oldCode={buggy} newCode={dirty} durationInFrames={MORPH} />
      </Sequence>
    );
  } else {
    inner = (
      <Sequence from={T.edit2} layout="none">
        <CodeTransition oldCode={dirty} newCode={clean} durationInFrames={MORPH} />
      </Sequence>
    );
  }
  return <div style={{minHeight}}>{inner}</div>;
};

export const Meme: React.FC<MemeProps> = (props) => {
  const frame = useCurrentFrame();

  const userText = 'this should add not subtract, pls fix';
  const ut = typed(frame, T.userStart, userText.length);

  const prefix = "You're absolutely right! ";
  const disc = 'Discombobulating...';
  const pt = typed(frame, T.agentStart, prefix.length);
  const dt = typed(frame, T.agentStart + prefix.length / CPS, disc.length);
  const discOpacity = interpolate(frame, [T.discGone, T.discGone + 10], [1, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });

  const cargoText = 'cargo install silence-cli';
  const ct = typed(frame, T.cargo, cargoText.length);
  const hookText = 'silence hook install';
  const ht = typed(frame, T.hook, hookText.length);

  return (
    <AbsoluteFill style={{background: gh.bg}}>
      <div
        style={{
          position: 'absolute',
          inset: 0,
          padding: '56px 60px',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {}
        <CodeArea {...props} />
        <div
          style={{
            height: 1,
            background: gh.windowBorder,
            margin: '24px 0 0',
            flexShrink: 0,
          }}
        />
        {}
        <div
          style={{
            flex: 1,
            minHeight: 0,
            overflow: 'hidden',
            display: 'flex',
            flexDirection: 'column',
            justifyContent: 'flex-end',
            gap: 18,
            paddingTop: 24,
          }}
        >
          {frame >= T.userStart && (
            <Bubble side="user" start={T.userStart}>
              {userText.slice(0, ut)}
              {ut < userText.length ? <Caret /> : null}
            </Bubble>
          )}
          {frame >= T.agentStart && (
            <Bubble side="agent" start={T.agentStart}>
              {prefix.slice(0, pt)}
              {discOpacity > 0 ? <Rainbow text={disc.slice(0, dt)} opacity={discOpacity} /> : null}
              {pt < prefix.length || (dt < disc.length && discOpacity > 0) ? <Caret /> : null}
            </Bubble>
          )}
          {frame >= T.king && (
            <KingBubble start={T.king} />
          )}
          {frame >= T.cargo && (
            <Bubble side="user" start={T.cargo}>
              {cargoText.slice(0, ct)}
              {ct < cargoText.length ? <Caret /> : null}
            </Bubble>
          )}
          {frame >= T.hook && (
            <Bubble side="user" start={T.hook}>
              {hookText.slice(0, ht)}
              {ht < hookText.length ? <Caret /> : null}
            </Bubble>
          )}
        </div>
      </div>
    </AbsoluteFill>
  );
};

const KingBubble: React.FC<{start: number}> = ({start}) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const s = spring({frame: frame - start, fps, config: {damping: 12, stiffness: 200}});
  return (
    <div
      style={{
        alignSelf: 'flex-end',
        opacity: s,
        transform: `scale(${interpolate(s, [0, 1], [0.6, 1])})`,
        transformOrigin: 'bottom right',
      }}
    >
      <Img
        src={staticFile('king.png')}
        style={{
          width: 300,
          height: 300,
          objectFit: 'cover',
          borderRadius: '22px 22px 8px 22px',
          display: 'block',
        }}
      />
    </div>
  );
};
