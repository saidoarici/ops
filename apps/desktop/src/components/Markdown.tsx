// Minimal, güvenli markdown görüntüleyici: React elemanları üretir,
// dangerouslySetInnerHTML KULLANMAZ. Kod bloğu, satır içi kod, kalın,
// başlık ve madde listelerini destekler — chat için yeterli alt küme.

import type { ReactNode } from "react";

function inline(text: string, keyBase: string): ReactNode[] {
  const parts = text.split(/(`[^`\n]+`|\*\*[^*\n]+\*\*)/g);
  return parts.map((part, i) => {
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) {
      return <code key={`${keyBase}-${i}`}>{part.slice(1, -1)}</code>;
    }
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) {
      return <strong key={`${keyBase}-${i}`}>{part.slice(2, -2)}</strong>;
    }
    return part;
  });
}

function renderTextBlock(block: string, keyBase: string): ReactNode[] {
  const out: ReactNode[] = [];
  const lines = block.split("\n");
  let listItems: string[] = [];
  let paragraph: string[] = [];
  let key = 0;

  const flushList = () => {
    if (listItems.length > 0) {
      out.push(
        <ul key={`${keyBase}-ul-${key++}`} className="md-list">
          {listItems.map((item, i) => (
            <li key={i}>{inline(item, `${keyBase}-li-${key}-${i}`)}</li>
          ))}
        </ul>,
      );
      listItems = [];
    }
  };
  const flushParagraph = () => {
    if (paragraph.length > 0) {
      out.push(
        <p key={`${keyBase}-p-${key++}`}>{inline(paragraph.join(" "), `${keyBase}-pi-${key}`)}</p>,
      );
      paragraph = [];
    }
  };

  for (const raw of lines) {
    const line = raw.trimEnd();
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    const bullet = /^\s*(?:[-*•]|\d+[.)])\s+(.*)$/.exec(line);
    if (line.trim() === "") {
      flushList();
      flushParagraph();
    } else if (heading) {
      flushList();
      flushParagraph();
      out.push(
        <div key={`${keyBase}-h-${key++}`} className={`md-h md-h${heading[1].length}`}>
          {inline(heading[2], `${keyBase}-hi-${key}`)}
        </div>,
      );
    } else if (bullet) {
      flushParagraph();
      listItems.push(bullet[1]);
    } else {
      flushList();
      paragraph.push(line.trim());
    }
  }
  flushList();
  flushParagraph();
  return out;
}

export function Markdown({ text }: { text: string }) {
  // Her ``` ayracı bir dil yakalaması üretir (kapanış dahil); içeride miyiz
  // bilgisini toggle ile izleriz: çift indeksler sırayla metin/kod olur.
  const segments = text.split(/```(\w*)\n?/);
  const nodes: ReactNode[] = [];
  let inCode = false;
  let lang = "";
  for (let i = 0; i < segments.length; i += 1) {
    if (i % 2 === 1) {
      lang = segments[i];
      inCode = !inCode;
      continue;
    }
    const seg = segments[i];
    if (inCode) {
      nodes.push(
        <pre key={`code-${i}`} className="md-code">
          {lang && <span className="md-lang">{lang}</span>}
          <code>{seg.replace(/\n$/, "")}</code>
        </pre>,
      );
    } else if (seg.trim() !== "") {
      nodes.push(...renderTextBlock(seg, `s${i}`));
    }
  }
  return <div className="md">{nodes}</div>;
}
