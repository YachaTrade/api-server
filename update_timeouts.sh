#!/bin/bash

# 500ms 타임아웃을 1000ms로 변경
find src/types -name "*.rs" -type f -exec sed -i '' 's/Duration::from_millis(500)/Duration::from_millis(1000)/g' {} \;

echo "Updated query timeouts from 500ms to 1000ms in all type files"

# 변경된 파일 확인
echo "Files updated:"
grep -l "Duration::from_millis(1000)" src/types/**/*.rs | wc -l