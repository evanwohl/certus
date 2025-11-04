;; echo.wat - Simple deterministic Wasm module that echoes input to output
;; This is the simplest possible deterministic function for testing
;;
;; Compiles to Wasm that:
;; 1. Reads input from memory at offset 0
;; 2. Copies it to output at offset 1024
;; 3. Returns the length
;;
;; To compile: wat2wasm echo.wat -o echo.wasm

(module
  ;; Memory: 1 page (64KB)
  (memory (export "memory") 1)

  ;; Input buffer at offset 0
  ;; Output buffer at offset 1024

  ;; Main entry point: process(inputPtr: i32, inputLen: i32) -> outputLen: i32
  (func $process (export "process") (param $inputPtr i32) (param $inputLen i32) (result i32)
    (local $i i32)
    (local $outputPtr i32)

    ;; Output pointer
    (local.set $outputPtr (i32.const 1024))

    ;; Copy input to output (echo)
    (local.set $i (i32.const 0))
    (block $break
      (loop $continue
        ;; Check if done
        (br_if $break (i32.ge_u (local.get $i) (local.get $inputLen)))

        ;; Copy byte: output[i] = input[i]
        (i32.store8
          (i32.add (local.get $outputPtr) (local.get $i))
          (i32.load8_u (i32.add (local.get $inputPtr) (local.get $i)))
        )

        ;; Increment counter
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $continue)
      )
    )

    ;; Return output length (same as input length)
    (local.get $inputLen)
  )
)
