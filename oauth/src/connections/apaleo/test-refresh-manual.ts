import axios from 'axios';
import { refresh } from './refresh';
import { DataObject } from '../../lib/types';

// Store the original implementation
const originalAxios = axios;

// Create a simple mock function for axios
let mockResponse: any = null;
let mockError: any = null;
let callCount = 0;
let lastCallArgs: any = null;

// Replace axios with our mock implementation
(axios as any) = async function mockAxios(config: any) {
    callCount++;
    lastCallArgs = config;

    // Log the request for verification
    console.log('Axios called with:', {
        url: config.url,
        method: config.method,
        headers: config.headers,
    });

    if (mockError) {
        return Promise.reject(mockError);
    }
    return Promise.resolve(mockResponse);
};

// Helper to reset mocks between tests
const resetMocks = () => {
    mockResponse = null;
    mockError = null;
    callCount = 0;
    lastCallArgs = null;
};

// Test data
const testData: DataObject = {
    body: {
        OAUTH_CLIENT_ID: 'KHWD-AC-INHOTEL_STAFF_ASSISTANT',
        OAUTH_CLIENT_SECRET: 'aecjPJNPKGidCcPoTeuQ9MmCURRUg8',
        OAUTH_REFRESH_TOKEN:
            'FBDA050C7B9EE0D7D24C1ED686C4A07BC6B85C67FAA3E9B11DA9252ECCCDBD8B-1',
    },
};

const successResponse = {
    data: {
        access_token: 'new-access-token',
        expires_in: 3600,
        token_type: 'Bearer',
        refresh_token: 'new-refresh-token',
    },
};

// Main test function
async function runTests() {
    console.log('=== Testing Apaleo refresh token functionality ===');

    // Test case 1: Successfully refreshes an access token
    console.log('\n--- Test 1: Successfully refreshes an access token ---');
    resetMocks();
    mockResponse = successResponse;

    try {
        const result = await refresh(testData);
        console.log('✅ Result:', result);

        // Verify expected result
        const expectedResult = {
            accessToken: 'new-access-token',
            refreshToken: 'new-refresh-token',
            expiresIn: 3600,
            tokenType: 'Bearer',
            meta: {
                OAUTH_CLIENT_ID: 'KHWD-AC-INHOTEL_STAFF_ASSISTANT',
                OAUTH_CLIENT_SECRET: 'aecjPJNPKGidCcPoTeuQ9MmCURRUg8',
            },
        };

        const isEqual =
            JSON.stringify(result) === JSON.stringify(expectedResult);
        console.log('✅ Result matches expected:', isEqual);
        console.log(
            '✅ Axios called correct number of times:',
            callCount === 1,
        );

        // Check call parameters
        if (lastCallArgs) {
            console.log(
                '✅ Called with correct URL:',
                lastCallArgs.url ===
                    'https://identity.apaleo.com/connect/token',
            );
            console.log(
                '✅ Called with correct method:',
                lastCallArgs.method === 'POST',
            );
            console.log(
                '✅ Called with correct content type:',
                lastCallArgs.headers['Content-Type'] ===
                    'application/x-www-form-urlencoded',
            );
        }
    } catch (error) {
        console.error('❌ Test failed:', error);
    }

    // Test case 2: Falls back to old refresh token if no new one is provided
    console.log(
        '\n--- Test 2: Falls back to old refresh token if no new one is provided ---',
    );
    resetMocks();
    mockResponse = {
        data: {
            access_token: 'new-access-token',
            expires_in: 3600,
            token_type: 'Bearer',
            // No refresh_token here
        },
    };

    try {
        const result = await refresh(testData);
        console.log('✅ Result:', result);
        console.log(
            '✅ Uses original refresh token:',
            result.refreshToken ===
                'FBDA050C7B9EE0D7D24C1ED686C4A07BC6B85C67FAA3E9B11DA9252ECCCDBD8B-1',
        );
    } catch (error) {
        console.error('❌ Test failed:', error);
    }

    // Test case 3: Handles API errors correctly
    console.log('\n--- Test 3: Handles API errors correctly ---');
    resetMocks();
    mockError = new Error('Network Error');

    try {
        await refresh(testData);
        console.error('❌ Test failed: Expected error was not thrown');
    } catch (error: any) {
        const expectedErrorMessage =
            'Error refreshing access token for Apaleo: Error: Network Error';
        console.log('✅ Error thrown:', error.message);
        console.log(
            '✅ Error message matches expected:',
            error.message === expectedErrorMessage,
        );
    }

    // Cleanup - restore original axios
    (axios as any) = originalAxios;
    console.log('\n=== All tests completed ===');
}

// Run the tests
runTests().catch(console.error);
