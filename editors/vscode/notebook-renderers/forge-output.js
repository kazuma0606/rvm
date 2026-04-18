"use strict";

const TABLE_MIME = "application/vnd.forge.table+json";
const HTML_MIME = "application/vnd.forge.html+json";
const PIPELINE_TRACE_MIME = "application/vnd.forge.pipeline-trace+json";

exports.activate = function activate() {
  return {
    renderOutputItem(outputItem, element) {
      clearElement(element);

      if (outputItem.mime === TABLE_MIME) {
        renderTable(outputItem, element);
        return;
      }

      if (outputItem.mime === HTML_MIME) {
        renderHtml(outputItem, element);
        return;
      }

      if (outputItem.mime === PIPELINE_TRACE_MIME) {
        renderPipelineTrace(outputItem, element);
      }
    }
  };
};

function clearElement(element) {
  while (element.firstChild) {
    element.removeChild(element.firstChild);
  }
}

function renderTable(outputItem, element) {
  const payload = outputItem.json();
  const columns = Array.isArray(payload.columns) ? payload.columns : inferColumns(payload.rows);
  const rows = Array.isArray(payload.rows) ? payload.rows : [];

  const wrapper = document.createElement("div");
  wrapper.style.overflowX = "auto";
  wrapper.style.margin = "0.25rem 0";

  const table = document.createElement("table");
  table.style.borderCollapse = "collapse";
  table.style.width = "100%";
  table.style.fontSize = "0.9rem";
  table.style.lineHeight = "1.45";
  table.style.background = "var(--vscode-editor-background)";
  table.style.color = "var(--vscode-editor-foreground)";

  const thead = document.createElement("thead");
  const headerRow = document.createElement("tr");
  for (const column of columns) {
    const th = document.createElement("th");
    th.textContent = String(column);
    applyCellStyle(th, true);
    headerRow.appendChild(th);
  }
  thead.appendChild(headerRow);
  table.appendChild(thead);

  const tbody = document.createElement("tbody");
  for (const row of rows) {
    const tr = document.createElement("tr");
    for (let index = 0; index < columns.length; index += 1) {
      const td = document.createElement("td");
      td.textContent = stringifyValue(Array.isArray(row) ? row[index] : undefined);
      applyCellStyle(td, false);
      tr.appendChild(td);
    }
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);

  wrapper.appendChild(table);
  element.appendChild(wrapper);
}

function renderHtml(outputItem, element) {
  const payload = outputItem.json();
  const iframe = document.createElement("iframe");
  iframe.setAttribute("sandbox", "allow-same-origin");
  iframe.style.width = "100%";
  iframe.style.border = "1px solid var(--vscode-panel-border)";
  iframe.style.borderRadius = "6px";
  iframe.style.background = "white";
  iframe.style.minHeight = "96px";
  iframe.srcdoc = typeof payload.value === "string" ? payload.value : "";
  iframe.addEventListener("load", () => {
    try {
      const body = iframe.contentDocument && iframe.contentDocument.body;
      if (!body) {
        return;
      }
      const height = Math.max(body.scrollHeight, 96);
      iframe.style.height = `${height}px`;
    } catch {
      iframe.style.height = "160px";
    }
  });
  element.appendChild(iframe);
}

function renderPipelineTrace(outputItem, element) {
  const payload = outputItem.json();
  let selectedStage = firstCorruptedStage(payload);

  const wrapper = document.createElement("div");
  wrapper.style.border = "1px solid var(--vscode-panel-border)";
  wrapper.style.borderRadius = "10px";
  wrapper.style.padding = "0.85rem";
  wrapper.style.margin = "0.35rem 0";
  wrapper.style.background = "color-mix(in srgb, var(--vscode-editor-background) 92%, black 8%)";

  const title = document.createElement("div");
  title.textContent = `Pipeline: ${stringValue(payload.pipeline_name)}`;
  title.style.fontWeight = "700";
  title.style.marginBottom = "0.7rem";
  wrapper.appendChild(title);

  const flow = document.createElement("div");
  flow.style.display = "flex";
  flow.style.flexWrap = "wrap";
  flow.style.alignItems = "center";
  flow.style.gap = "0.5rem";
  wrapper.appendChild(flow);

  const detailHost = document.createElement("div");
  detailHost.style.marginTop = "0.9rem";

  const stages = Array.isArray(payload.stages) ? payload.stages : [];
  stages.forEach((stage, index) => {
    flow.appendChild(
      renderStageCard(stage, payload, detailHost, stageName => {
        selectedStage = stageName;
      })
    );
    if (index < stages.length - 1) {
      const arrow = document.createElement("div");
      arrow.textContent = "->";
      arrow.style.opacity = "0.7";
      flow.appendChild(arrow);
    }
  });

  const sourceBlock = document.createElement("pre");
  sourceBlock.style.margin = "0.9rem 0 0";
  sourceBlock.style.padding = "0.75rem";
  sourceBlock.style.borderRadius = "8px";
  sourceBlock.style.background = "var(--vscode-textCodeBlock-background)";
  sourceBlock.style.whiteSpace = "pre-wrap";
  sourceBlock.style.fontFamily = "var(--vscode-editor-font-family)";
  sourceBlock.style.fontSize = "0.9rem";
  renderSourceLines(payload, sourceBlock);
  wrapper.appendChild(sourceBlock);

  renderStageDetail(payload, detailHost, selectedStage);
  wrapper.appendChild(detailHost);

  if (Array.isArray(payload.corruptions) && payload.corruptions.length > 0) {
    const details = document.createElement("div");
    details.style.marginTop = "0.85rem";
    details.style.padding = "0.75rem";
    details.style.borderRadius = "8px";
    details.style.background = "color-mix(in srgb, #7a1f1f 22%, var(--vscode-editor-background) 78%)";
    details.style.border = "1px solid color-mix(in srgb, #d94a4a 55%, var(--vscode-panel-border) 45%)";

    const detailsTitle = document.createElement("div");
    detailsTitle.textContent = `Corruption Details (${payload.corruptions.length})`;
    detailsTitle.style.fontWeight = "700";
    detailsTitle.style.marginBottom = "0.45rem";
    details.appendChild(detailsTitle);

    const list = document.createElement("ul");
    list.style.margin = "0";
    list.style.paddingLeft = "1.1rem";
    for (const corruption of payload.corruptions) {
      const item = document.createElement("li");
      item.textContent = `#${numberValue(corruption.index)} [${stringValue(corruption.stage)}] ${stringValue(corruption.reason)}`;
      list.appendChild(item);
    }
    details.appendChild(list);
    wrapper.appendChild(details);
  }

  element.appendChild(wrapper);
}

function renderStageCard(stage, payload, detailHost, onSelect) {
  const card = document.createElement("div");
  const corrupted = numberValue(stage.corrupted);
  card.style.minWidth = "92px";
  card.style.padding = "0.55rem 0.7rem";
  card.style.borderRadius = "8px";
  card.style.border = corrupted > 0
    ? "1px solid #d94a4a"
    : "1px solid var(--vscode-panel-border)";
  card.style.background = corrupted > 0
    ? "color-mix(in srgb, #7a1f1f 18%, var(--vscode-editor-background) 82%)"
    : "var(--vscode-editor-background)";
  card.style.cursor = "pointer";

  const name = document.createElement("div");
  name.textContent = stringValue(stage.name);
  name.style.fontWeight = "600";
  card.appendChild(name);

  const count = document.createElement("div");
  count.textContent = `${numberValue(stage.out)} records`;
  count.style.opacity = "0.82";
  count.style.fontSize = "0.85rem";
  card.appendChild(count);

  if (corrupted > 0) {
    const badge = document.createElement("div");
    badge.textContent = `${corrupted} corrupted`;
    badge.style.marginTop = "0.25rem";
    badge.style.color = "#ff8f8f";
    badge.style.fontSize = "0.8rem";
    badge.style.fontWeight = "700";
    card.appendChild(badge);
  }

  card.addEventListener("click", () => {
    const stageName = stringValue(stage.name);
    onSelect(stageName);
    renderStageDetail(payload, detailHost, stageName);
  });

  return card;
}

function renderStageDetail(payload, element, stageName) {
  clearElement(element);
  if (!stageName) {
    return;
  }

  const recordsByStage = payload.records_by_stage && typeof payload.records_by_stage === "object"
    ? payload.records_by_stage
    : {};
  const rows = Array.isArray(recordsByStage[stageName]) ? recordsByStage[stageName] : [];
  if (rows.length === 0) {
    return;
  }

  const title = document.createElement("div");
  title.textContent = `Stage Detail: ${stageName}`;
  title.style.fontWeight = "700";
  title.style.marginBottom = "0.45rem";
  element.appendChild(title);

  const columns = inferObjectColumns(rows);
  const table = document.createElement("table");
  table.style.borderCollapse = "collapse";
  table.style.width = "100%";
  table.style.fontSize = "0.88rem";

  const thead = document.createElement("thead");
  const headerRow = document.createElement("tr");
  for (const column of columns) {
    const th = document.createElement("th");
    th.textContent = column;
    applyCellStyle(th, true);
    headerRow.appendChild(th);
  }
  thead.appendChild(headerRow);
  table.appendChild(thead);

  const tbody = document.createElement("tbody");
  for (const row of rows) {
    const tr = document.createElement("tr");
    const reason = typeof row.reason === "string" ? row.reason : "";
    if (reason.length > 0) {
      tr.style.background = "color-mix(in srgb, #7a1f1f 16%, transparent 84%)";
    }
    for (const column of columns) {
      const td = document.createElement("td");
      td.textContent = stringifyValue(row[column]);
      applyCellStyle(td, false);
      tr.appendChild(td);
    }
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  element.appendChild(table);
}

function applyCellStyle(cell, header) {
  cell.style.padding = "0.45rem 0.65rem";
  cell.style.border = "1px solid var(--vscode-panel-border)";
  cell.style.textAlign = "left";
  cell.style.verticalAlign = "top";
  if (header) {
    cell.style.fontWeight = "600";
    cell.style.background = "var(--vscode-sideBar-background)";
  }
}

function inferColumns(rows) {
  if (!Array.isArray(rows)) {
    return [];
  }
  const width = rows.reduce((max, row) => {
    if (!Array.isArray(row)) {
      return max;
    }
    return Math.max(max, row.length);
  }, 0);
  return Array.from({ length: width }, (_, index) => `col_${index}`);
}

function renderSourceLines(payload, container) {
  const snippet = typeof payload.source_snippet === "string" ? payload.source_snippet : "";
  const lines = snippet.split(/\r?\n/);
  const corruptedLines = new Map();
  const stages = Array.isArray(payload.stages) ? payload.stages : [];
  for (const stage of stages) {
    const line = numberValue(stage.line);
    const corrupted = numberValue(stage.corrupted);
    if (line > 0 && corrupted > 0) {
      corruptedLines.set(line, corrupted);
    }
  }

  lines.forEach((lineText, index) => {
    const line = document.createElement("div");
    const lineNo = index + 1;
    line.textContent = lineText;
    if (corruptedLines.has(lineNo)) {
      line.style.background = "color-mix(in srgb, #7a1f1f 28%, transparent 72%)";
      line.style.borderLeft = "3px solid #d94a4a";
      line.style.paddingLeft = "0.55rem";
      line.title = `${corruptedLines.get(lineNo)} corrupted`;
    }
    container.appendChild(line);
  });
}

function stringifyValue(value) {
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "object") {
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }
  return String(value);
}

function stringValue(value) {
  return typeof value === "string" ? value : "";
}

function numberValue(value) {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function firstCorruptedStage(payload) {
  const stages = Array.isArray(payload.stages) ? payload.stages : [];
  const found = stages.find(stage => numberValue(stage.corrupted) > 0);
  return found ? stringValue(found.name) : null;
}

function inferObjectColumns(rows) {
  const seen = new Set();
  for (const row of rows) {
    if (!row || typeof row !== "object" || Array.isArray(row)) {
      continue;
    }
    for (const key of Object.keys(row)) {
      seen.add(key);
    }
  }
  return Array.from(seen);
}
