<script setup lang="ts">
import { computed, ref } from "vue";
import DocsSearch from "./components/DocsSearch.vue";
import DocsSidebar from "./components/DocsSidebar.vue";
import TablePage from "./components/TablePage.vue";
import WarningBanner from "./components/WarningBanner.vue";
import WikiIndex from "./components/WikiIndex.vue";
import "./docs.css";
import { groupBySchema, groupByTableGroup } from "./docsIndex";
import { renderNote } from "./renderNote";
import type { DocTable, SchemaSnapshot } from "./types";

const props = defineProps<{
  snapshot: SchemaSnapshot;
}>();

// Grouping is computed once here and handed to both the sidebar and the index,
// so the two can never disagree about what the sections are.
const mode = ref<"schema" | "group">(props.snapshot.groups.length > 0 ? "group" : "schema");
const activeKey = ref<string | null>(null);

function tableKey(table: DocTable): string {
  return table.schema ? `${table.schema}.${table.name}` : table.name;
}

const sections = computed(() => (mode.value === "schema" ? groupBySchema(props.snapshot) : groupByTableGroup(props.snapshot)));

const activeTable = computed(() => props.snapshot.tables.find((table) => tableKey(table) === activeKey.value) ?? null);

/** "table" whenever a table is open, "index" otherwise. */
const view = computed<"index" | "table">(() => (activeTable.value === null ? "index" : "table"));

const activeGroup = computed(() => {
  const groupId = activeTable.value?.groupId;
  if (!groupId) {
    return null;
  }
  return props.snapshot.groups.find((group) => group.id === groupId) ?? null;
});

function open(key: string): void {
  // A key naming no table leaves the reader where they are rather than
  // dropping them on a blank page.
  if (props.snapshot.tables.some((table) => tableKey(table) === key)) {
    activeKey.value = key;
  }
}
</script>

<template>
  <div class="flex h-full min-h-0 bg-background text-foreground">
    <DocsSidebar :sections="sections" :mode="mode" :active-key="activeKey" @update:mode="mode = $event" @select="open" @home="activeKey = null" />

    <main class="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
      <header class="flex flex-wrap items-start justify-between gap-3">
        <div class="flex flex-col gap-1">
          <h1 class="text-base font-semibold">{{ snapshot.project.name }}</h1>
          <p class="text-xs text-muted-foreground">
            {{ snapshot.project.databaseType }}<template v-if="snapshot.project.database"> · {{ snapshot.project.database }}</template> · {{ snapshot.tables.length }} tables · generated {{ snapshot.project.generatedAt }}
          </p>
        </div>
        <DocsSearch :snapshot="snapshot" @select="open" />
      </header>

      <WarningBanner :warnings="snapshot.warnings" />

      <div v-if="view === 'index'" class="flex flex-col gap-4">
        <div v-if="snapshot.project.note" class="text-sm text-muted-foreground" v-html="renderNote(snapshot.project.note)"></div>
        <WikiIndex :sections="sections" @select="open" />
      </div>

      <TablePage v-else-if="activeTable" :table="activeTable" :relationships="snapshot.relationships" :group="activeGroup" @select="open" />
    </main>
  </div>
</template>
