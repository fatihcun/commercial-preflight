import assert from "node:assert/strict";
import test from "node:test";
import {
  LOCALE_STORAGE_KEY,
  initializeLocale,
  resolveInitialLocale,
  setLocale,
  t,
} from "../src/i18n.ts";

function storage(initial = {}) {
  const values = new Map(Object.entries(initial));
  return {
    getItem(key) { return values.get(key) ?? null; },
    setItem(key, value) { values.set(key, value); },
    value(key) { return values.get(key); },
  };
}

test("selecting Turkish exposes Turkish desktop strings", () => {
  const local = storage();
  setLocale("tr", local);
  assert.equal(t("nav.orderInbox"), "Sipariş Gelen Kutusu");
  assert.equal(t("button.finishSetup"), "Kurulumu Tamamla");
  assert.equal(t("sidebar.shadowMode"), "Çalışma modu: Gölge");
  assert.equal(t("empty.ordersHint"), "İlk siparişi eklemek için sağ üstteki ‘Sipariş Ekle’ düğmesini kullanın.");
});

test("selecting English exposes English desktop strings", () => {
  const local = storage();
  setLocale("en", local);
  assert.equal(t("nav.orderInbox"), "Order Inbox");
  assert.equal(t("button.finishSetup"), "Finish Setup");
});

test("manual locale choice is persisted locally", () => {
  const local = storage();
  setLocale("tr", local);
  assert.equal(local.value(LOCALE_STORAGE_KEY), "tr");
  assert.equal(resolveInitialLocale(local, "en-US"), "tr");
});

test("Turkish Windows locale defaults to Turkish without persisting", () => {
  const local = storage();
  assert.equal(initializeLocale(local, "tr-TR"), "tr");
  assert.equal(local.value(LOCALE_STORAGE_KEY), undefined);
});

test("non-Turkish Windows locale defaults to English", () => {
  assert.equal(resolveInitialLocale(storage(), "de-DE"), "en");
  assert.equal(resolveInitialLocale(storage(), "en-US"), "en");
});
