// Auto's order (issue #86): a dialog where the codecs Auto picks from are
// put in the order wanted, by dragging a row or with its up / down buttons
// (the keyboard's way), and set back to the sound quality order. OK saves
// settings.auto_order (empty for the default order); the core then picks
// the first codec in it the device offers, and a chosen codec it does not
// offer falls back down it.

import { createEffect, createSignal, For } from "solid-js";
import { t } from "../i18n";
import { AUTO_ORDER, codecSpec, isDefaultOrder, type CodecId } from "../lib/codecs";
import { Button, Dialog, IconButton } from "./controls";
import { Icon } from "./Icon";

/** `list` with the item at `from` moved to `to` */
export function moved<T>(list: readonly T[], from: number, to: number): T[] {
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

export function AutoOrderDialog(props: {
  open: boolean;
  order: readonly CodecId[];
  onSave: (order: CodecId[]) => void;
  onCancel: () => void;
}) {
  const [list, setList] = createSignal<CodecId[]>([...props.order]);
  const [dragging, setDragging] = createSignal<number | null>(null);
  // Each opening starts from the order saved
  createEffect(() => {
    if (props.open) setList([...props.order]);
  });
  const move = (from: number, to: number) => {
    if (to < 0 || to >= list().length || from === to) return;
    setList(moved(list(), from, to));
  };
  const name = (id: CodecId) => t(codecSpec(id).label);

  return (
    <Dialog
      open={props.open}
      title={t("ui.codecs.order_title")}
      confirm={t("modal.ok")}
      cancel={t("modal.cancel")}
      onConfirm={() => props.onSave(list())}
      onCancel={props.onCancel}
    >
      <p class="hint">{t("ui.codecs.order_hint")}</p>
      <ol class="auto-order" aria-label={t("ui.codecs.order_title")}>
        <For each={list()}>
          {(id, i) => (
            <li
              class="auto-order-row"
              classList={{ dragging: dragging() === i() }}
              draggable="true"
              onDragStart={(e) => {
                setDragging(i());
                e.dataTransfer?.setData("text/plain", id);
                if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
              }}
              onDragOver={(e) => {
                e.preventDefault();
                const from = dragging();
                // Moves as the pointer passes over a row: what is dropped is
                // what was shown
                if (from !== null && from !== i()) {
                  move(from, i());
                  setDragging(i());
                }
              }}
              onDrop={(e) => e.preventDefault()}
              onDragEnd={() => setDragging(null)}
            >
              <Icon name="navigation" class="auto-order-grip" />
              <span class="auto-order-rank">{i() + 1}</span>
              <span class="auto-order-name">{name(id)}</span>
              <span class="auto-order-moves">
                <IconButton
                  icon="chevron_down"
                  class="auto-order-up"
                  label={t("ui.codecs.order_up", name(id))}
                  disabled={i() === 0}
                  onClick={() => move(i(), i() - 1)}
                />
                <IconButton
                  icon="chevron_down"
                  label={t("ui.codecs.order_down", name(id))}
                  disabled={i() === list().length - 1}
                  onClick={() => move(i(), i() + 1)}
                />
              </span>
            </li>
          )}
        </For>
      </ol>
      <Button icon="arrow_reset" disabled={isDefaultOrder(list())} onClick={() => setList([...AUTO_ORDER])}>
        {t("ui.codecs.order_reset")}
      </Button>
    </Dialog>
  );
}
