/* Tiny types module, shaped like a real stdint.h. */
#ifndef TINY_STDINT_H
#define TINY_STDINT_H 1

#include <stddef.h>

typedef signed int   int32_t;
typedef unsigned int uint32_t;

#define INT8_MAX (127)

typedef struct vec {
  int32_t x;
  int32_t y;
  int32_t z;
} vec_t;

enum status {
  STATUS_OK = 0,
  STATUS_ERROR = 1,
};

/* The dot product of two vectors. */
int vec_dot(const vec_t *a, const vec_t *b) {
  return a->x * b->x + a->y * b->y + a->z * b->z;
}

#endif /* TINY_STDINT_H */
