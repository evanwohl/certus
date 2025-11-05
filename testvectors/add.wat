;; Simple integer addition
(module
  (memory 1)
  (export "memory" (memory 0))
  (export "main" (func $add))

  (func $add (param $ptr i32) (param $len i32) (result i32)
    ;; Read two i32 values from input
    (local $a i32)
    (local $b i32)

    (local.set $a (i32.load (local.get $ptr)))
    (local.set $b (i32.load offset=4 (local.get $ptr)))

    ;; Compute sum
    (i32.store offset=256 (i32.const 0)
      (i32.add (local.get $a) (local.get $b)))

    ;; Return pointer to result
    (i32.const 256)
  )
)