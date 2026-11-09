/**
 * One component per widget type. `WidgetBody` picks it; each renders its
 * own plate (WidgetPlate) so the legend's readout can come from its data.
 */

import type { ComponentType } from "react";

import type {
  AnyProfileWidget,
  BoardData,
  WidgetType,
} from "@/lib/profile-board";

import type { BoardCtx } from "../board-format";
import { BadgesWidget } from "./Badges";
import { CountdownWidget } from "./Countdown";
import { CustomWidget } from "./Custom";
import { GuestbookWidget } from "./Guestbook";
import { LinksWidget } from "./Links";
import { ListeningWidget } from "./Listening";
import { MoodWidget } from "./Mood";
import { NowPlayingWidget } from "./NowPlaying";
import { PhotosWidget } from "./Photos";
import { QuoteWidget } from "./Quote";
import { ServersWidget } from "./Servers";
import type { WidgetProps } from "./shared";
import { TopGamesWidget } from "./TopGames";
import { VoiceHoursWidget } from "./VoiceHours";

const WIDGET_COMPONENTS = {
  now_playing: NowPlayingWidget,
  top_games: TopGamesWidget,
  voice_hours: VoiceHoursWidget,
  listening: ListeningWidget,
  badges: BadgesWidget,
  servers: ServersWidget,
  quote: QuoteWidget,
  guestbook: GuestbookWidget,
  mood: MoodWidget,
  photos: PhotosWidget,
  links: LinksWidget,
  countdown: CountdownWidget,
  custom: CustomWidget,
} satisfies { [T in WidgetType]: ComponentType<WidgetProps<T>> };

/** The plate and content of one widget, as its type draws it. */
export function WidgetBody({
  widget,
  data,
  ctx,
}: {
  widget: AnyProfileWidget;
  data: BoardData;
  ctx: BoardCtx;
}) {
  const Component = WIDGET_COMPONENTS[
    widget.type
  ] as ComponentType<WidgetProps>;
  return <Component widget={widget} data={data} ctx={ctx} />;
}

export { WidgetEmpty, WidgetImage, type WidgetProps } from "./shared";
