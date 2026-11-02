#include <iostream>
#include <string>

/** Add two integers.
 * @param first First value.
 * @param second Second value.
 * @return The sum.
 */
int combine(int first, int second) { return first + second; }
/** Add two decimal values. */
double combine(double first, double second) { return first + second; }
/** Print a message. */
void combine(const char* message) { std::cout << message << '\n'; }

int main() {
    std::string title = "disco";
    // Type comb below, accept it, then type ( to inspect overloads.
    // Or try title.repl for std::string::replace.

    return 0;
}
