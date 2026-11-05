import { useEffect, useState } from 'react';

export const asyncFeedbackDelay = 300;

/** 只延迟视觉提示，不延迟执行或解锁。key 改变时重新计算本次操作的等待时间。 */
export function useDelayedBusy(
  busy: boolean,
  key: string | number = 'default',
  delay = asyncFeedbackDelay,
) {
  const [shown, setShown] = useState<string | number | null>(null);
  useEffect(() => {
    if (!busy) return;
    const timer = setTimeout(() => setShown(key), delay);
    return () => {
      clearTimeout(timer);
      setShown(null);
    };
  }, [busy, key, delay]);
  return busy && shown === key;
}
