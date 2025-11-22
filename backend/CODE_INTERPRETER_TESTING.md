# Testing Code Interpreter in Chat

This guide explains how to test automatic code execution in chat conversations with the Rust backend.

## What's Implemented

The Rust backend now has **full code interpreter support** that automatically detects and executes code blocks in chat responses!

### Features:
- ✅ Automatic code block detection in streaming responses
- ✅ Multi-language support (Python, JavaScript, Bash, Ruby, PHP, Go, Rust)
- ✅ Real-time execution during chat streaming
- ✅ Execution results displayed inline
- ✅ Error handling and display
- ✅ Security via sandbox isolation
- ✅ Configurable timeout

## Prerequisites

### 1. Start Sandbox Executor
```bash
cd sandbox-executor
cargo run --release
```
- Running at: `http://localhost:8090`
- Verify: `curl http://localhost:8090/api/v1/health`

### 2. Start Rust Backend
```bash
cd rust-backend
cargo run
```
- Running at: `http://localhost:8168`
- Verify: `curl http://localhost:8168/health`

### 3. Start Frontend
```bash
npm run dev
```
- Running at: `http://localhost:5173`

## Configuration

### Method 1: Environment Variables (.env)

Add to your `.env` file:

```env
# Enable code interpreter
ENABLE_CODE_INTERPRETER=true
CODE_INTERPRETER_ENGINE=sandbox
CODE_INTERPRETER_SANDBOX_URL=http://localhost:8090
CODE_INTERPRETER_SANDBOX_TIMEOUT=60
```

### Method 2: Admin Settings UI

1. Navigate to: `http://localhost:5173/admin/settings/code-execution`

2. **Code Interpreter Section:**
   - Enable: "Enable Code Interpreter"
   - Engine: Select "sandbox"
   - Sandbox Executor URL: `http://localhost:8090`
   - Timeout: `60` (seconds)

3. Click **Save**

4. **Restart the Rust backend** to apply changes

## How It Works

### 1. Automatic Detection

When a model generates a code block in markdown format:

````markdown
Here's a Python script:

```python
print("Hello, World!")
result = 2 + 2
print(f"2 + 2 = {result}")
```
````

The Rust backend will:
1. Detect the code block as it's streaming
2. Extract the language (`python`) and code
3. Execute it in the sandbox
4. Stream the results back to the chat

### 2. Supported Languages

| Language   | Syntax            | Notes                          |
|------------|-------------------|--------------------------------|
| Python     | ```python         | Python 3.x                     |
| JavaScript | ```javascript     | Node.js                        |
| Bash       | ```bash           | Shell scripts                  |
| Ruby       | ```ruby           | Ruby interpreter               |
| PHP        | ```php            | PHP CLI                        |
| Go         | ```go             | Requires compilation           |
| Rust       | ```rust           | Requires compilation           |

### 3. Language Aliases

These are automatically normalized:
- `py` → `python`
- `js` → `javascript`
- `sh`, `shell` → `bash`
- `rb` → `ruby`
- `rs` → `rust`

## Testing Scenarios

### Test 1: Simple Python Calculation

**Prompt:** "Write a Python script to calculate the factorial of 5"

**Expected Response:**
```python
def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n-1)

result = factorial(5)
print(f"Factorial of 5 is: {result}")
```

**Expected Output:**
```
Code Execution Result (123ms)

Output:
Factorial of 5 is: 120
```

### Test 2: JavaScript Array Operations

**Prompt:** "Show me a JavaScript example of sorting an array"

**Expected Response:**
```javascript
const numbers = [64, 34, 25, 12, 22, 11, 90];
const sorted = numbers.sort((a, b) => a - b);
console.log("Sorted array:", sorted);
```

**Expected Output:**
```
Code Execution Result (87ms)

Output:
Sorted array: [ 11, 12, 22, 25, 34, 64, 90 ]
```

### Test 3: Multiple Code Blocks

**Prompt:** "Show me examples in both Python and JavaScript for printing hello"

The model might generate:

````markdown
Here are examples:

Python:
```python
print("Hello from Python!")
```

JavaScript:
```javascript
console.log("Hello from JavaScript!");
```
````

Both code blocks will be executed automatically, with results shown inline.

### Test 4: Error Handling

**Prompt:** "Write a Python script with a division by zero error"

**Expected Response:**
```python
result = 10 / 0
print(result)
```

**Expected Output:**
```
Code Execution Result (45ms)

Errors:
Traceback (most recent call last):
  File "script.py", line 1, in <module>
    result = 10 / 0
ZeroDivisionError: division by zero

Exit Code: 1
```

### Test 5: File Operations (Sandbox Isolated)

**Prompt:** "Write a Python script to create and read a file"

**Expected Response:**
```python
with open('/workspace/test.txt', 'w') as f:
    f.write('Hello, Sandbox!')

with open('/workspace/test.txt', 'r') as f:
    content = f.read()
    print(f"File content: {content}")
```

**Expected Output:**
```
Code Execution Result (156ms)

Output:
File content: Hello, Sandbox!
```

### Test 6: Long-Running Code (Timeout)

**Prompt:** "Write a Python script that sleeps for 100 seconds"

```python
import time
print("Sleeping...")
time.sleep(100)
print("Done!")
```

**Expected Behavior:**
- Execution will timeout after 60 seconds (default)
- Error message displayed

## Verification Steps

### 1. Check Rust Backend Logs

When code execution happens, you'll see:

```
[INFO] 🔍 Detected complete code block: python (45 bytes)
[INFO] 🔒 Executing code block: python (45 bytes)
[INFO] ✅ Code execution completed: success (123ms)
```

### 2. Check Sandbox Executor Logs

```
[INFO] Received code execution request
[INFO] Creating container with security profile
[INFO] Executing code in container
[INFO] Container execution completed: exit_code=0
```

### 3. Check Frontend

- Code block appears in chat
- Execution results appear below the code
- Results are formatted with output/errors sections
- Execution time is displayed

## Troubleshooting

### Code Not Executing

**Symptom:** Code blocks appear but don't execute

**Solutions:**
1. Check `ENABLE_CODE_INTERPRETER=true` in config
2. Check `CODE_INTERPRETER_ENGINE=sandbox`
3. Verify sandbox-executor is running: `curl http://localhost:8090/api/v1/health`
4. Check Rust backend logs for errors
5. Restart Rust backend after config changes

### "Sandbox executor client not initialized"

**Solution:**
- Set `ENABLE_CODE_EXECUTION=true` in addition to `ENABLE_CODE_INTERPRETER=true`
- The sandbox client is initialized when code execution is enabled

### Code Blocks Not Detected

**Possible Causes:**
1. Model not generating proper markdown code blocks (missing backticks)
2. Language not supported (check supported languages list)
3. Code block not complete (streaming still in progress)

**Check logs for:**
```
[DEBUG] Detected code block: python
```

### Execution Timeout

**Default timeout:** 60 seconds

**To change:**
```env
CODE_INTERPRETER_SANDBOX_TIMEOUT=120
```

Or via Admin UI.

### Security Restrictions

The sandbox has security restrictions:
- No network access (by default)
- Read-only root filesystem
- Limited capabilities
- Resource limits (CPU, memory)

Some operations may fail due to these restrictions.

## Advanced Testing

### Testing with curl (Direct API)

For comparison, you can test the direct execution endpoint:

```bash
curl -X POST http://localhost:8168/api/v1/utils/code/execute \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer YOUR_TOKEN' \
  -d '{
    "code": "print(\"Direct execution test\")",
    "language": "python"
  }'
```

### Testing Streaming

Use WebSocket or Socket.IO connection to see real-time streaming of both code blocks and execution results.

### Testing with Different Models

Different models behave differently:
- **GPT-4**: Often generates well-formatted code blocks
- **Claude**: Good at explaining code before generating
- **Llama**: May need explicit prompting for code blocks

**Tip:** Explicitly ask for code blocks:
- "Write a Python script in a code block"
- "Show me code examples with proper markdown syntax"

## Performance Metrics

Expected performance:
- **Code detection:** < 1ms per chunk
- **Python execution:** 50-500ms depending on code
- **JavaScript execution:** 30-300ms
- **Container startup:** ~100-500ms (first execution)
- **Subsequent executions:** Faster (container reuse)

## Configuration Reference

### Environment Variables

```env
# Code Interpreter
ENABLE_CODE_INTERPRETER=true
CODE_INTERPRETER_ENGINE=sandbox
CODE_INTERPRETER_SANDBOX_URL=http://localhost:8090
CODE_INTERPRETER_SANDBOX_TIMEOUT=60

# Code Execution (also needed for sandbox client init)
ENABLE_CODE_EXECUTION=true
CODE_EXECUTION_ENGINE=sandbox
CODE_EXECUTION_SANDBOX_URL=http://localhost:8090
CODE_EXECUTION_SANDBOX_TIMEOUT=60
```

### Comparison: Python vs Rust Backend

| Feature | Python Backend | Rust Backend | Status |
|---------|---------------|--------------|--------|
| Code block detection | ✅ | ✅ | Complete |
| Streaming execution | ✅ | ✅ | Complete |
| Multi-language | ✅ | ✅ | Complete |
| Sandbox isolation | ✅ | ✅ | Complete |
| Real-time results | ✅ | ✅ | Complete |
| Error handling | ✅ | ✅ | Complete |
| Timeout support | ✅ | ✅ | Complete |

## Example Chat Flow

1. **User:** "Calculate fibonacci sequence in Python"

2. **Assistant:** "Here's a Python implementation:"
   ````
   ```python
   def fibonacci(n):
       if n <= 1:
           return n
       return fibonacci(n-1) + fibonacci(n-2)
   
   for i in range(10):
       print(f"fibonacci({i}) = {fibonacci(i)}")
   ```
   ````

3. **System (Automatic Execution):**
   ```
   Code Execution Result (234ms)
   
   Output:
   fibonacci(0) = 0
   fibonacci(1) = 1
   fibonacci(2) = 1
   fibonacci(3) = 2
   fibonacci(4) = 3
   fibonacci(5) = 5
   fibonacci(6) = 8
   fibonacci(7) = 13
   fibonacci(8) = 21
   fibonacci(9) = 34
   ```

4. **User:** "Great! Now do it in JavaScript"

5. **Assistant:** (generates JavaScript code, automatically executed)

**Automatic Code Execution:** Works in real-time during chat streaming  
**Multi-Language Support:** Python, JavaScript, Bash, Ruby, PHP, Go, Rust  
**Secure Execution:** Sandbox isolation with resource limits  
**Error Handling:** Errors displayed inline with helpful messages  
**Performance:** Fast execution with real-time streaming  
**Configuration:** Easy setup via UI or environment variables  

## Next Steps

- Try different languages and prompts
- Test error scenarios
- Monitor performance and logs
- Adjust timeout for longer executions
- Explore multi-block execution

For more details on the implementation, see:
- `rust-backend/src/middleware/code_interpreter.rs` - Code detection logic
- `rust-backend/src/utils/chat_completion.rs` - Streaming integration
- `sandbox-executor/README.md` - Sandbox executor documentation

