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
import {FireBurn} from './FireBurn';
import {gh, FONT} from './theme';

export type MemeProps = {
  buggy: HighlightedCode;
  dirty: HighlightedCode;
  clean: HighlightedCode;
};

const T = {
  userStart: 8,
  agentStart: 48,
  discStart: 70,
  edit1: 88,
  discGone: 102,
  king: 114,
  cargo: 150,
  hook: 176,
  strip: 196,
  fire: 214,
};
const XFADE = 8;
const BURNHOLD = 66;
const BURN = BURNHOLD + 30;
const COLLAPSE = T.fire + BURNHOLD;
const CPS = 1.5;
export const TOTAL = COLLAPSE + 38;

const LINE = Math.round(30 * 1.5);
const CODE_LINES = 5;
const PAD_TOP = 56;
const PAD_LEFT = 60;

const typed = (frame: number, start: number, len: number) =>
  Math.max(0, Math.min(len, Math.floor((frame - start) * CPS)));

const usePop = (start: number) => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const s = spring({
    frame: frame - start,
    fps,
    config: {damping: 13, stiffness: 210, mass: 0.6},
  });
  const opacity = interpolate(s, [0, 0.5], [0, 1], {extrapolateRight: 'clamp'});
  const scale = interpolate(s, [0, 1], [0.9, 1]);
  const y = interpolate(s, [0, 1], [10, 0]);
  return {opacity, transform: `translateY(${y}px) scale(${scale})`};
};

const Caret: React.FC = () => {
  const frame = useCurrentFrame();
  const on = Math.floor(frame / 9) % 2 === 0;
  return (
    <span
      style={{
        display: 'inline-block',
        width: '0.55em',
        height: '1.05em',
        transform: 'translateY(0.18em)',
        background: 'currentColor',
        opacity: on ? 0.8 : 0,
        marginLeft: 1,
      }}
    />
  );
};

const Bubble: React.FC<{
  side: 'user' | 'agent';
  start: number;
  children: React.ReactNode;
}> = ({side, start, children}) => {
  const pop = usePop(start);
  const user = side === 'user';
  return (
    <div
      style={{
        alignSelf: user ? 'flex-end' : 'flex-start',
        transformOrigin: user ? 'bottom right' : 'bottom left',
        maxWidth: 660,
        background: user ? '#0969da' : '#f1f2f4',
        color: user ? '#fff' : gh.text,
        fontFamily: FONT,
        fontSize: 30,
        lineHeight: 1.45,
        padding: '14px 22px',
        borderRadius: user ? '22px 22px 8px 22px' : '22px 22px 22px 8px',
        whiteSpace: 'pre-wrap',
        ...pop,
      }}
    >
      {children}
    </div>
  );
};

const Rainbow: React.FC<{text: string}> = ({text}) => {
  const frame = useCurrentFrame();
  return (
    <>
      {text.split('').map((ch, i) => {
        const hue = (((i * 26 - frame * 6) % 360) + 360) % 360;
        return (
          <span
            key={i}
            style={{display: 'inline-block', whiteSpace: 'pre', color: `hsl(${hue}, 88%, 52%)`}}
          >
            {ch}
          </span>
        );
      })}
    </>
  );
};

const CodeArea: React.FC<MemeProps> = ({buggy, dirty, clean}) => {
  const frame = useCurrentFrame();
  const minHeight = CODE_LINES * LINE;
  let content: React.ReactNode;
  if (frame < T.edit1) {
    content = <StaticCode code={buggy} />;
  } else if (frame < T.edit1 + XFADE) {
    const a = interpolate(frame, [T.edit1, T.edit1 + XFADE], [0, 1]);
    content = (
      <div style={{position: 'relative'}}>
        <div style={{opacity: 1 - a}}>
          <StaticCode code={buggy} />
        </div>
        <div style={{position: 'absolute', inset: 0, opacity: a}}>
          <StaticCode code={dirty} />
        </div>
      </div>
    );
  } else if (frame < COLLAPSE) {
    content = <StaticCode code={dirty} />;
  } else {
    content = (
      <Sequence from={COLLAPSE} layout="none">
        <CodeTransition oldCode={dirty} newCode={clean} durationInFrames={20} />
      </Sequence>
    );
  }
  return <div style={{minHeight}}>{content}</div>;
};

export const Meme: React.FC<MemeProps> = (props) => {
  const frame = useCurrentFrame();

  const userText = 'this should add not subtract, pls fix';
  const ut = typed(frame, T.userStart, userText.length);

  const agentText = "You're absolutely right!";
  const at = typed(frame, T.agentStart, agentText.length);

  const discA = interpolate(frame, [T.discStart, T.discStart + 8], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  const discFade = interpolate(frame, [T.discGone, T.discGone + 8], [1, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  const discVisible = frame >= T.discStart && discFade > 0;

  const cargoText = 'cargo install silence-cli';
  const ct = typed(frame, T.cargo, cargoText.length);
  const hookText = 'silence hook install';
  const ht = typed(frame, T.hook, hookText.length);
  const stripText = 'silence strip add.ts';
  const st = typed(frame, T.strip, stripText.length);
  const terminalPop = usePop(T.cargo);

  return (
    <AbsoluteFill style={{background: gh.bg}}>
      <div
        style={{
          position: 'absolute',
          inset: 0,
          padding: `${PAD_TOP}px ${PAD_LEFT}px`,
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        <CodeArea {...props} />
        <div style={{height: 1, background: gh.windowBorder, marginTop: 24}} />
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
            <div
              style={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'flex-start',
                gap: 7,
              }}
            >
              <Bubble side="agent" start={T.agentStart}>
                {agentText.slice(0, at)}
                {at < agentText.length ? <Caret /> : null}
              </Bubble>
              {discVisible && (
                <div
                  style={{
                    fontFamily: FONT,
                    fontSize: 23,
                    paddingLeft: 6,
                    opacity: discA * discFade,
                  }}
                >
                  <Rainbow text="Discombobulating..." />
                </div>
              )}
            </div>
          )}

          {frame >= T.king && <KingBubble start={T.king} />}

          {frame >= T.cargo && (
            <div
              style={{
                alignSelf: 'flex-end',
                transformOrigin: 'bottom right',
                width: 500,
                boxSizing: 'border-box',
                background: '#1f2328',
                color: '#e6edf3',
                fontFamily: FONT,
                fontSize: 27,
                lineHeight: 1.6,
                padding: '18px 26px',
                borderRadius: 14,
                whiteSpace: 'nowrap',
                boxShadow: '0 12px 34px rgba(0,0,0,0.22)',
                ...terminalPop,
              }}
            >
              <div>
                <span style={{color: '#7ee787'}}>$ </span>
                {cargoText.slice(0, ct)}
                {ct < cargoText.length ? <Caret /> : null}
              </div>
              {frame >= T.hook && (
                <div>
                  <span style={{color: '#7ee787'}}>$ </span>
                  {hookText.slice(0, ht)}
                  {ht < hookText.length ? <Caret /> : null}
                </div>
              )}
              {frame >= T.strip && (
                <div>
                  <span style={{color: '#7ee787'}}>$ </span>
                  {stripText.slice(0, st)}
                  {st < stripText.length ? <Caret /> : null}
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      <BurningComment />
      <FireBurn
        start={T.fire}
        duration={BURN}
        top={PAD_TOP + 2 * LINE}
        left={86}
        width={812}
        height={LINE}
      />
    </AbsoluteFill>
  );
};

const COMMENT = '  // Adds numbers, does not subtract. Returns.';
const CHAR_W = 16.3;
const COMMENT_ROW_Y = PAD_TOP + 2 * LINE + LINE / 2 - 2;
const seed = (i: number) => {
  const x = Math.sin(i * 91.7 + 13.3) * 43758.5453;
  return x - Math.floor(x);
};

const BurningComment: React.FC = () => {
  const frame = useCurrentFrame();
  if (frame < T.fire || frame > COLLAPSE + 6) return null;
  const prog = interpolate(frame, [T.fire, T.fire + BURNHOLD - 8], [0, 1], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });
  const layerFade = interpolate(frame, [COLLAPSE - 1, COLLAPSE + 3], [1, 0], {
    extrapolateLeft: 'clamp',
    extrapolateRight: 'clamp',
  });

  return (
    <div style={{position: 'absolute', inset: 0, pointerEvents: 'none', opacity: layerFade}}>
      {COMMENT.split('').map((ch, i) => {
        if (!/[A-Za-z]/.test(ch)) return null;
        const threshold = 0.08 + seed(i) * 0.82;
        const ip = interpolate(prog, [threshold, threshold + 0.05], [0, 1], {
          extrapolateLeft: 'clamp',
          extrapolateRight: 'clamp',
        });
        if (ip <= 0) return null;
        const cx = PAD_LEFT + (i + 0.5) * CHAR_W;
        const flick = 0.86 + 0.14 * Math.sin(frame * 0.7 + i * 1.7);
        return (
          <div
            key={i}
            style={{
              position: 'absolute',
              left: cx,
              top: COMMENT_ROW_Y,
              transform: 'translate(-50%, -50%)',
            }}
          >
            {}
            <div
              style={{
                position: 'absolute',
                left: '50%',
                top: '50%',
                width: CHAR_W + 4,
                height: 32,
                background: gh.bg,
                transform: 'translate(-50%, -50%)',
                opacity: ip,
              }}
            />
            <span
              style={{
                position: 'relative',
                display: 'block',
                fontSize: 25,
                lineHeight: 1,
                transform: `scale(${ip * flick})`,
              }}
            >
              🔥
            </span>
          </div>
        );
      })}
    </div>
  );
};

const KingBubble: React.FC<{start: number}> = ({start}) => {
  const pop = usePop(start);
  return (
    <div
      style={{
        alignSelf: 'flex-end',
        transformOrigin: 'bottom right',
        ...pop,
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
