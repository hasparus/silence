import {loadFont} from '@remotion/fonts';
import {staticFile} from 'remotion';
import {FONT} from './theme';

export const fontsReady = Promise.all([
  loadFont({
    family: FONT,
    url: staticFile('fonts/ComicMono.ttf'),
    weight: '400',
  }),
  loadFont({
    family: FONT,
    url: staticFile('fonts/ComicMono-Bold.ttf'),
    weight: '700',
  }),
]);
