#include <sys/ptrace.h> /* ptrace */
#include <sys/user.h> /* struct user */
#include <stddef.h> /* offsetof */
#include <stdio.h> /* printf */

int main() {
  printf("debug0: %ld\n", offsetof(struct user, u_debugreg[0]));
  printf("debug7: %ld\n", offsetof(struct user, u_debugreg[7]));
}
