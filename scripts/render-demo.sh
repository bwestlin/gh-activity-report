#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
output_dir="$repo_root/assets"
demo_tmp=$(mktemp -d)

cleanup() {
  find "$demo_tmp" -type f -delete
  rmdir "$demo_tmp"
}
trap cleanup EXIT

font=/usr/share/fonts/truetype/firacode/FiraCode-Regular.ttf
if [[ ! -f "$font" ]]; then
  font=DejaVu-Sans-Mono
fi
ui_font=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf
if [[ ! -f "$ui_font" ]]; then
  ui_font=DejaVu-Sans
fi

mkdir -p "$output_dir"

render_frame() {
  local frame=$1
  local cursor=${2:-true}
  local output_name=${3:-frame-$frame-$cursor}
  local command=${4:-'$ gh-activity-report yesterday'}
  local status_x=930
  local -a draw=(
    -size 1200x720 "xc:#ffffff"
    -stroke "#30363d" -strokewidth 2 -fill "#0d1117" -draw "roundrectangle 1,1 1198,718 14,14"
    -stroke none -fill "#161b22" -draw "roundrectangle 2,2 1197,47 13,13 rectangle 2,28 1197,48"
    -fill "#ff5f56" -draw "circle 25,24 31,24"
    -fill "#ffbd2e" -draw "circle 46,24 52,24"
    -fill "#27c93f" -draw "circle 67,24 73,24"
    -font "$font" -pointsize 16 -fill "#8b949e" -gravity NorthWest
    -annotate +486+15 "gh-activity-report — standup"
  )

  if (( frame >= 1 )); then
    draw+=( -pointsize 20 -fill "#e6edf3" -annotate +34+68 "$command" )
  fi
  if (( frame >= 2 )); then
    draw+=(
      -pointsize 22 -fill "#56d4dd" -annotate +34+112 'GitHub progress for @octocat'
      -pointsize 18 -fill "#e6edf3" -annotate +34+148 'Yesterday (2026-10-26) · 3 pull requests · 2 issues · 14 activity signals'
    )
  fi
  if (( frame >= 3 )); then
    draw+=(
      -pointsize 18 -fill "#c678dd" -annotate +34+199 'PULL REQUESTS MOVED FORWARD'
      -fill "#61afef" -annotate +34+236 '1. acme/web#184'
      -fill "#e6edf3" -annotate +235+236 'Cache authenticated API responses'
      -fill "#98c379" -annotate +${status_x}+236 'Merged'
      -fill "#7d8590" -annotate +34+268 '   2 commits, 1 review, 2 comments, 2 PR updates'
      -fill "#61afef" -annotate +34+303 '2. acme/api#77'
      -fill "#e6edf3" -annotate +225+303 'Add request correlation IDs'
      -fill "#56d4dd" -annotate +${status_x}+303 'Open'
      -fill "#7d8590" -annotate +34+335 '   1 commit, 1 review, 1 comment'
      -fill "#61afef" -annotate +34+370 '3. acme/design#42'
      -fill "#e6edf3" -annotate +250+370 'Refresh settings navigation'
      -fill "#e5c07b" -annotate +${status_x}+370 'Draft'
    )
  fi
  if (( frame >= 4 )); then
    draw+=(
      -pointsize 20 -fill "#56d4dd" -annotate +34+425 '1. Cache authenticated API responses'
      -pointsize 17 -fill "#61afef" -annotate +34+458 'acme/web#184'
      -fill "#e6edf3" -annotate +215+458 'authored by you · feature/cache-auth -> main'
      -fill "#98c379" -annotate +${status_x}+458 'Merged'
      -fill "#7d8590" -annotate +34+488 'Progress: 2 commits, 1 review, 2 comments, 2 PR updates'
    )
  fi
  if (( frame >= 5 )); then
    draw+=(
      -pointsize 17 -fill "#61afef" -annotate +34+530 'Related issues'
      -fill "#e6edf3" -annotate +54+562 'Closes: acme/web#163 — Repeated API calls after sign-in'
      -fill "#98c379" -annotate +${status_x}+562 'Closed'
      -fill "#61afef" -annotate +34+604 'Commits (2)'
      -fill "#e6edf3" -annotate +54+636 'a41dc92 Cache session-scoped requests'
      -fill "#7d8590" -annotate +705+636 '(14:32)'
      -fill "#e6edf3" -annotate +54+668 'dd77b10 Add cache invalidation coverage'
      -fill "#7d8590" -annotate +705+668 '(15:08)'
    )
  fi
  if [[ "$cursor" == true ]]; then
    draw+=( -fill "#e6edf3" -draw "rectangle 34,688 46,691" )
  fi
  draw+=( -pointsize 13 -fill "#484f58" -annotate +1030+696 'mock data' )

  magick "${draw[@]}" "$demo_tmp/$output_name.png"
}

render_variant_frame() {
  local name=$1
  local command=$2
  local format=$3
  local -a draw=(
    -size 1200x720 "xc:#ffffff"
    -stroke "#30363d" -strokewidth 2 -fill "#0d1117" -draw "roundrectangle 1,1 1198,718 14,14"
    -stroke none -fill "#161b22" -draw "roundrectangle 2,2 1197,47 13,13 rectangle 2,28 1197,48"
    -fill "#ff5f56" -draw "circle 25,24 31,24"
    -fill "#ffbd2e" -draw "circle 46,24 52,24"
    -fill "#27c93f" -draw "circle 67,24 73,24"
    -font "$font" -pointsize 16 -fill "#8b949e" -gravity NorthWest
    -annotate +486+15 "gh-activity-report — standup"
    -pointsize 20 -fill "#e6edf3" -annotate +34+68 "$command"
  )

  case "$format" in
    this-week)
      draw+=(
        -pointsize 22 -fill "#56d4dd" -annotate +34+112 'GitHub progress for @octocat'
        -pointsize 18 -fill "#e6edf3" -annotate +34+148 'This week (2026-10-26 to 2026-10-27) · 4 pull requests · 3 issues · 22 activity signals'
        -fill "#c678dd" -annotate +34+199 'PULL REQUESTS MOVED FORWARD'
        -fill "#61afef" -annotate +34+236 '1. acme/web#184'
        -fill "#e6edf3" -annotate +235+236 'Cache authenticated API responses'
        -fill "#98c379" -annotate +930+236 'Merged'
        -fill "#7d8590" -annotate +34+268 '   4 commits, 1 review, 3 comments, 2 PR updates'
        -fill "#61afef" -annotate +34+303 '2. acme/api#77'
        -fill "#e6edf3" -annotate +225+303 'Add request correlation IDs'
        -fill "#56d4dd" -annotate +930+303 'Open'
        -fill "#7d8590" -annotate +34+335 '   2 commits, 2 reviews, 2 comments'
        -fill "#61afef" -annotate +34+370 '3. acme/design#42'
        -fill "#e6edf3" -annotate +250+370 'Refresh settings navigation'
        -fill "#e5c07b" -annotate +930+370 'Draft'
        -fill "#7d8590" -annotate +34+402 '   1 commit, 1 comment, 1 PR update'
        -pointsize 20 -fill "#56d4dd" -annotate +34+459 '1. Cache authenticated API responses'
        -pointsize 17 -fill "#61afef" -annotate +34+496 'acme/web#184'
        -fill "#e6edf3" -annotate +215+496 'authored by you · feature/cache-auth -> main'
        -fill "#98c379" -annotate +930+496 'Merged'
        -fill "#7d8590" -annotate +34+526 'Progress: 4 commits, 1 review, 3 comments, 2 PR updates'
        -fill "#61afef" -annotate +34+572 'Related issues'
        -fill "#e6edf3" -annotate +54+605 'Closes: acme/web#163 — Repeated API calls after sign-in'
        -fill "#98c379" -annotate +930+605 'Closed'
      )
      ;;
    markdown)
      draw+=(
        -pointsize 16 -fill "#e6edf3" -annotate +34+112 '# GitHub progress for @octocat'
        -annotate +34+149 '**Last week** (2026-10-19 to 2026-10-25) — 2 pull requests and 1 issue moved forward'
        -annotate +34+176 '(9 activity signals)'
        -fill "#c678dd" -annotate +34+220 '## Pull requests moved forward'
        -fill "#e6edf3" -annotate +34+257 '1. [acme/web#184 — Cache authenticated API responses]('
        -fill "#61afef" -annotate +604+257 'https://github.com/acme/web/pull/184'
        -fill "#e6edf3" -annotate +954+257 ')'
        -annotate +52+286 '— **Merged** — 2 commits, 1 review, 2 comments, 2 PR updates'
        -annotate +34+326 '2. [acme/api#77 — Add request correlation IDs]('
        -fill "#61afef" -annotate +530+326 'https://github.com/acme/api/pull/77'
        -fill "#e6edf3" -annotate +861+326 ')'
        -annotate +52+355 '— **Open** — 1 commit, 1 review, 1 comment'
        -fill "#c678dd" -annotate +34+404 '## 1. [Cache authenticated API responses](https://github.com/acme/web/pull/184)'
        -fill "#e6edf3" -annotate +34+441 '`acme/web#184` · **Merged** · Authored by you · `feature/cache-auth` → `main`'
        -annotate +34+478 '**Progress:** 2 commits, 1 review, 2 comments, 2 PR updates'
        -fill "#c678dd" -annotate +34+527 '### Related issues'
        -fill "#e6edf3" -annotate +34+564 '- **Closes:** [acme/web#163 — Repeated API calls after sign-in](...) — **Closed**'
      )
      ;;
    console)
      draw+=(
        -pointsize 22 -fill "#56d4dd" -annotate +34+112 'GitHub progress for @octocat'
        -pointsize 18 -fill "#e6edf3" -annotate +34+148 'Custom range (2026-10-01 to 2026-10-15) · 2 pull requests · 1 issue · 9 activity signals'
        -fill "#c678dd" -annotate +34+199 'PULL REQUESTS MOVED FORWARD'
        -fill "#61afef" -annotate +34+236 '1. acme/api#77'
        -fill "#e6edf3" -annotate +225+236 'Add request correlation IDs'
        -fill "#56d4dd" -annotate +930+236 'Open'
        -fill "#7d8590" -annotate +34+268 '   1 commit, 1 review, 1 comment'
        -fill "#61afef" -annotate +34+303 '2. acme/web#161'
        -fill "#e6edf3" -annotate +235+303 'Reduce duplicate profile requests'
        -fill "#98c379" -annotate +930+303 'Merged'
        -fill "#7d8590" -annotate +34+335 '   2 commits, 1 PR update'
        -pointsize 20 -fill "#56d4dd" -annotate +34+392 '1. Add request correlation IDs'
        -pointsize 17 -fill "#61afef" -annotate +34+429 'acme/api#77'
        -fill "#e6edf3" -annotate +215+429 'contributed to · feature/request-id -> main'
        -fill "#56d4dd" -annotate +930+429 'Open'
        -fill "#7d8590" -annotate +34+459 'Progress: 1 commit, 1 review, 1 comment'
        -fill "#61afef" -annotate +34+505 'Reviews (1)'
        -fill "#e6edf3" -annotate +54+538 'Approved changes after request-ID tests passed'
        -fill "#7d8590" -annotate +820+538 '(10:41)'
        -fill "#61afef" -annotate +34+581 'Comments (1)'
        -fill "#e6edf3" -annotate +54+614 'Suggested documenting the proxy header behavior'
        -fill "#7d8590" -annotate +820+614 '(10:47)'
      )
      ;;
    html)
      draw+=(
        -stroke "#30363d" -strokewidth 1 -fill "#161b22" -draw "roundrectangle 34,108 1165,686 10,10"
        -stroke none -fill "#21262d" -draw "roundrectangle 35,109 1164,151 9,9 rectangle 35,132 1164,152"
        -fill "#8b949e" -font "$font" -pointsize 14 -annotate +64+124 'standup.html'
        -font "$ui_font" -pointsize 28 -fill "#e6edf3" -annotate +70+182 'GitHub progress for '
        -fill "#a371f7" -annotate +351+182 '@octocat'
        -pointsize 15 -fill "#8b949e" -annotate +70+222 'Yesterday (2026-10-26) · 3 pull requests · 2 issues · 14 activity signals'
        -stroke "#30363d" -strokewidth 1 -fill "#0d1117" -draw "roundrectangle 70,258 1128,441 10,10"
        -stroke none -font "$ui_font" -pointsize 21 -fill "#e6edf3" -annotate +94+284 'Pull requests moved forward'
        -pointsize 15 -fill "#58a6ff" -annotate +112+328 'acme/web#184 — Cache authenticated API responses'
        -stroke "#3fb950" -strokewidth 1 -fill none -draw "roundrectangle 815,315 884,340 12,12"
        -stroke none -pointsize 11 -fill "#3fb950" -annotate +827+320 'MERGED'
        -pointsize 13 -fill "#8b949e" -annotate +128+354 '2 commits, 1 review, 2 comments, 2 PR updates'
        -pointsize 15 -fill "#58a6ff" -annotate +112+393 'acme/api#77 — Add request correlation IDs'
        -stroke "#58a6ff" -strokewidth 1 -fill none -draw "roundrectangle 815,380 868,405 12,12"
        -stroke none -pointsize 11 -fill "#58a6ff" -annotate +828+385 'OPEN'
        -pointsize 13 -fill "#8b949e" -annotate +128+419 '1 commit, 1 review, 1 comment'
        -stroke "#30363d" -strokewidth 1 -fill "#0d1117" -draw "roundrectangle 70,466 1128,650 10,10"
        -stroke none -pointsize 21 -fill "#e6edf3" -annotate +94+491 '1. '
        -fill "#58a6ff" -annotate +122+491 'Cache authenticated API responses'
        -pointsize 14 -fill "#8b949e" -annotate +94+535 'acme/web#184 · Authored by you · feature/cache-auth → main'
        -pointsize 14 -fill "#e6edf3" -annotate +94+571 'Progress: '
        -fill "#8b949e" -annotate +170+571 '2 commits, 1 review, 2 comments, 2 PR updates'
        -pointsize 17 -fill "#e6edf3" -annotate +94+609 'Related issues'
        -pointsize 14 -fill "#58a6ff" -annotate +112+637 'Closes: acme/web#163 — Repeated API calls after sign-in'
        -stroke "#3fb950" -strokewidth 1 -fill none -draw "roundrectangle 815,620 883,645 12,12"
        -stroke none -pointsize 11 -fill "#3fb950" -annotate +825+625 'CLOSED'
      )
      ;;
  esac

  draw+=( -pointsize 13 -font "$font" -fill "#484f58" -annotate +1030+696 'mock data' )
  magick "${draw[@]}" "$demo_tmp/$name.png"
}

render_frame 0 true
render_variant_frame option-this-week \
  '$ gh-activity-report this-week' this-week
render_variant_frame option-last-week \
  '$ gh-activity-report last-week --format markdown' markdown
render_variant_frame option-date-range \
  '$ gh-activity-report --since 2026-10-01 --until 2026-10-15' console
render_variant_frame option-html \
  '$ gh-activity-report yesterday --format html > standup.html' html
render_frame 1 true
render_frame 2 true
render_frame 3 true
render_frame 4 true
render_frame 5 true
render_frame 5 false

magick \
  -delay 30 "$demo_tmp/frame-0-true.png" \
  -delay 65 "$demo_tmp/frame-1-true.png" \
  -delay 65 "$demo_tmp/frame-2-true.png" \
  -delay 75 "$demo_tmp/frame-3-true.png" \
  -delay 75 "$demo_tmp/frame-4-true.png" \
  -delay 250 "$demo_tmp/frame-5-true.png" \
  -delay 45 "$demo_tmp/frame-5-false.png" \
  -delay 45 "$demo_tmp/frame-5-true.png" \
  -delay 230 "$demo_tmp/option-this-week.png" \
  -delay 230 "$demo_tmp/option-last-week.png" \
  -delay 230 "$demo_tmp/option-date-range.png" \
  -delay 260 "$demo_tmp/option-html.png" \
  -loop 0 -layers Optimize "$output_dir/gh-activity-report-demo.gif"

echo "Rendered $output_dir/gh-activity-report-demo.gif"
