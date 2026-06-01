import {loadFont} from '@remotion/fonts';
import {staticFile} from 'remotion';
import {FONT} from './theme';

export const fontsReady = Promise.all([
  loadFont({
    family: FONT,
    url: staticFile('fonts/CommitMono-Regular.otf'),
    weight: '400',
  }),
  loadFont({
    family: FONT,
    url: staticFile('fonts/CommitMono-Bold.otf'),
    weight: '700',
  }),
]);
