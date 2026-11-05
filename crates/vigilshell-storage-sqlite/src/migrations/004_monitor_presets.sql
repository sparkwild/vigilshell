UPDATE preferences SET monitor_full_seconds=CASE WHEN monitor_sample_seconds IN (30,60) THEN 120 ELSE 60 END,
monitor_sample_seconds=CASE WHEN monitor_sample_seconds IN (5,10) THEN 5 ELSE 15 END
WHERE monitor_sample_seconds IN (5,10,30,60) AND monitor_full_seconds IN (60,120,300);
